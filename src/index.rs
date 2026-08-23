use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use tantivy::collector::TopDocs;
use tantivy::query::QueryParser;
use tantivy::schema::{Schema, Value, FAST, STORED, STRING, TEXT};
use tantivy::{doc, Index, IndexReader, IndexWriter, TantivyDocument, Term};
use walkdir::WalkDir;

use crate::wiki::{wiki_root, INDEX_FILE};

const TITLE_BOOST: f32 = 3.0;

const STATE_DIR: &str = ".llog";
const INDEX_DIR: &str = "index";
const STATE_FILE: &str = "state.json";

#[derive(Serialize, Deserialize, Default)]
struct State {
    files: HashMap<String, u64>,
}

fn state_path() -> PathBuf {
    PathBuf::from(STATE_DIR).join(STATE_FILE)
}

fn index_path() -> PathBuf {
    PathBuf::from(STATE_DIR).join(INDEX_DIR)
}

fn load_state() -> Result<State> {
    let path = state_path();
    if !path.exists() {
        return Ok(State::default());
    }
    let raw = fs::read_to_string(&path).with_context(|| format!("reading {}", path.display()))?;
    Ok(serde_json::from_str(&raw).unwrap_or_default())
}

fn save_state(state: &State) -> Result<()> {
    fs::create_dir_all(STATE_DIR)?;
    let raw = serde_json::to_string_pretty(state)?;
    fs::write(state_path(), raw).with_context(|| format!("writing {}", state_path().display()))
}

fn build_schema() -> (Schema, tantivy::schema::Field, tantivy::schema::Field, tantivy::schema::Field) {
    let mut builder = Schema::builder();
    let path_field = builder.add_text_field("path", STRING | STORED | FAST);
    let title_field = builder.add_text_field("title", TEXT | STORED);
    let body_field = builder.add_text_field("body", TEXT | STORED);
    (builder.build(), path_field, title_field, body_field)
}

fn mtime_secs(path: &Path) -> Result<u64> {
    let meta = fs::metadata(path)?;
    let modified = meta.modified()?;
    Ok(modified.duration_since(UNIX_EPOCH)?.as_secs())
}

fn title_of(contents: &str, fallback: &str) -> String {
    contents
        .lines()
        .find_map(|l| l.strip_prefix("# "))
        .map(str::to_string)
        .unwrap_or_else(|| fallback.to_string())
}

fn open_index_synced() -> Result<(Index, tantivy::schema::Field, tantivy::schema::Field, tantivy::schema::Field)> {
    fs::create_dir_all(index_path())?;
    let (schema, path_field, title_field, body_field) = build_schema();

    let index = if index_path().join("meta.json").exists() {
        Index::open_in_dir(index_path())?
    } else {
        Index::create_in_dir(index_path(), schema.clone())?
    };

    let mut state = load_state()?;
    let mut writer: IndexWriter = index.writer(50_000_000)?;
    let mut changed = false;

    let mut seen = HashMap::new();
    if wiki_root().exists() {
        for entry in WalkDir::new(wiki_root())
            .into_iter()
            .filter_map(|e| e.ok())
            .filter(|e| e.path().extension().is_some_and(|ext| ext == "md"))
        {
            let path = entry.path().to_path_buf();
            let rel = path
                .strip_prefix(wiki_root())
                .unwrap_or(&path)
                .to_string_lossy()
                .to_string();
            if rel == INDEX_FILE {
                continue;
            }
            let mtime = mtime_secs(&path)?;
            seen.insert(rel.clone(), mtime);

            if state.files.get(&rel) == Some(&mtime) {
                continue;
            }

            let contents = fs::read_to_string(&path)
                .with_context(|| format!("reading {}", path.display()))?;
            let title = title_of(&contents, &rel);

            writer.delete_term(Term::from_field_text(path_field, &rel));
            writer.add_document(doc!(
                path_field => rel.clone(),
                title_field => title,
                body_field => contents,
            ))?;
            changed = true;
        }
    }

    for rel in state.files.keys() {
        if !seen.contains_key(rel) {
            writer.delete_term(Term::from_field_text(path_field, rel));
            changed = true;
        }
    }

    if changed {
        writer.commit()?;
        state.files = seen;
        save_state(&state)?;
    }

    Ok((index, path_field, title_field, body_field))
}

pub struct Hit {
    pub path: String,
}

pub fn build() -> Result<()> {
    open_index_synced()?;
    Ok(())
}

pub fn search(query_str: &str, limit: usize) -> Result<Vec<Hit>> {
    let (index, path_field, title_field, body_field) = open_index_synced()?;
    let reader: IndexReader = index.reader()?;
    let searcher = reader.searcher();

    let mut parser = QueryParser::for_index(&index, vec![title_field, body_field]);
    parser.set_field_boost(title_field, TITLE_BOOST);
    let query = parser.parse_query(query_str)?;

    let top_docs = searcher.search(&query, &TopDocs::with_limit(limit))?;

    let mut hits = Vec::new();
    for (_score, doc_address) in top_docs {
        let doc: TantivyDocument = searcher.doc(doc_address)?;
        let path = doc
            .get_first(path_field)
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_string();

        hits.push(Hit { path });
    }

    Ok(hits)
}
