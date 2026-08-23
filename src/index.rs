use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result};
use chrono::{Local, NaiveDateTime, TimeZone};
use serde::{Deserialize, Serialize};
use tantivy::collector::TopDocs;
use tantivy::query::QueryParser;
use tantivy::schema::{Field, Schema, Value, FAST, STORED, STRING, TEXT};
use tantivy::{doc, Index, IndexReader, IndexWriter, TantivyDocument, Term};
use walkdir::WalkDir;

use crate::wiki::{wiki_root, INDEX_FILE, LOG_DIR};

const TITLE_BOOST: f32 = 3.0;
const LOG_HALF_LIFE_DAYS: f64 = 90.0;
const CANDIDATE_POOL: usize = 50;

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

struct Fields {
    path: Field,
    title: Field,
    body: Field,
    mtime: Field,
}

fn build_schema() -> (Schema, Fields) {
    let mut builder = Schema::builder();
    let path = builder.add_text_field("path", STRING | STORED | FAST);
    let title = builder.add_text_field("title", TEXT | STORED);
    let body = builder.add_text_field("body", TEXT | STORED);
    let mtime = builder.add_u64_field("mtime", FAST | STORED);
    (builder.build(), Fields { path, title, body, mtime })
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

fn open_index_synced() -> Result<(Index, Fields)> {
    fs::create_dir_all(index_path())?;
    let (schema, fields) = build_schema();

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

            writer.delete_term(Term::from_field_text(fields.path, &rel));
            writer.add_document(doc!(
                fields.path => rel.clone(),
                fields.title => title,
                fields.body => contents,
                fields.mtime => mtime,
            ))?;
            changed = true;
        }
    }

    for rel in state.files.keys() {
        if !seen.contains_key(rel) {
            writer.delete_term(Term::from_field_text(fields.path, rel));
            changed = true;
        }
    }

    if changed {
        writer.commit()?;
        state.files = seen;
        save_state(&state)?;
    }

    Ok((index, fields))
}

pub struct Hit {
    pub path: String,
}

pub fn build() -> Result<()> {
    open_index_synced()?;
    Ok(())
}

fn log_entry_time(rel_path: &str) -> Option<u64> {
    let stem = Path::new(rel_path).file_stem()?.to_str()?;
    let candidates = [stem.to_string()]
        .into_iter()
        .chain(stem.rsplit_once('_').filter(|(_, n)| n.chars().all(|c| c.is_ascii_digit())).map(|(base, _)| base.to_string()));

    for candidate in candidates {
        if let Ok(naive) = NaiveDateTime::parse_from_str(&candidate, "%Y-%m-%d-%H-%M") {
            if let Some(dt) = Local.from_local_datetime(&naive).single() {
                return Some(dt.timestamp() as u64);
            }
        }
    }
    None
}

fn log_decay(mtime: u64, now: u64) -> f32 {
    let age_days = now.saturating_sub(mtime) as f64 / 86_400.0;
    0.5f64.powf(age_days / LOG_HALF_LIFE_DAYS) as f32
}

pub fn search(query_str: &str, limit: usize) -> Result<Vec<Hit>> {
    let (index, fields) = open_index_synced()?;
    let reader: IndexReader = index.reader()?;
    let searcher = reader.searcher();

    let mut parser = QueryParser::for_index(&index, vec![fields.title, fields.body]);
    parser.set_field_boost(fields.title, TITLE_BOOST);
    let query = parser.parse_query(query_str)?;

    let candidate_limit = limit.max(CANDIDATE_POOL);
    let top_docs = searcher.search(&query, &TopDocs::with_limit(candidate_limit))?;
    let now = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs();

    let mut scored: Vec<(f32, String)> = Vec::new();
    for (score, doc_address) in top_docs {
        let doc: TantivyDocument = searcher.doc(doc_address)?;
        let path = doc
            .get_first(fields.path)
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_string();
        let adjusted = if path.starts_with(&format!("{LOG_DIR}/")) {
            let mtime = log_entry_time(&path)
                .or_else(|| doc.get_first(fields.mtime).and_then(|v| v.as_u64()))
                .unwrap_or(now);
            score * log_decay(mtime, now)
        } else {
            score
        };
        scored.push((adjusted, path));
    }

    scored.sort_by(|a, b| b.0.total_cmp(&a.0));
    Ok(scored.into_iter().take(limit).map(|(_, path)| Hit { path }).collect())
}
