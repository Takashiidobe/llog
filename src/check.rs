use std::fs;
use std::path::{Component, Path, PathBuf};

use anyhow::{Context, Result};
use walkdir::WalkDir;

use crate::wiki::wiki_root;

pub struct DanglingLink {
    pub file: PathBuf,
    pub target: String,
}

fn extract_link_targets(contents: &str) -> Vec<String> {
    let chars: Vec<char> = contents.chars().collect();
    let mut links = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        if chars[i] == '['
            && let Some(close) = chars[i + 1..].iter().position(|&c| c == ']')
        {
            let text_end = i + 1 + close;
            if text_end + 1 < chars.len()
                && chars[text_end + 1] == '('
                && let Some(paren_close) = chars[text_end + 2..].iter().position(|&c| c == ')')
            {
                let target_start = text_end + 2;
                let target_end = target_start + paren_close;
                links.push(chars[target_start..target_end].iter().collect());
                i = target_end + 1;
                continue;
            }
        }
        i += 1;
    }
    links
}

fn normalize_target(raw: &str) -> Option<String> {
    let target = raw.split_whitespace().next()?.trim();
    if target.is_empty()
        || target.starts_with('#')
        || target.contains("://")
        || target.starts_with("mailto:")
    {
        return None;
    }
    let target = target.split('#').next().unwrap_or(target);
    if target.is_empty() {
        None
    } else {
        Some(target.to_string())
    }
}

fn lexical_normalize(path: &Path) -> PathBuf {
    let mut out = Vec::new();
    for comp in path.components() {
        match comp {
            Component::ParentDir => {
                out.pop();
            }
            Component::CurDir => {}
            other => out.push(other),
        }
    }
    out.into_iter().collect()
}

pub fn find_dangling_links() -> Result<Vec<DanglingLink>> {
    let root = wiki_root();
    let mut dangling = Vec::new();
    if !root.exists() {
        return Ok(dangling);
    }

    for entry in WalkDir::new(&root)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().is_some_and(|ext| ext == "md"))
    {
        let path = entry.path();
        let contents =
            fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
        let dir = path.parent().unwrap_or_else(|| Path::new(""));

        for raw in extract_link_targets(&contents) {
            let Some(target) = normalize_target(&raw) else {
                continue;
            };
            let joined = match target.strip_prefix('/') {
                Some(stripped) => root.join(stripped),
                None => dir.join(&target),
            };
            if !lexical_normalize(&joined).exists() {
                dangling.push(DanglingLink {
                    file: path.to_path_buf(),
                    target: raw,
                });
            }
        }
    }

    dangling.sort_by(|a, b| a.file.cmp(&b.file).then_with(|| a.target.cmp(&b.target)));
    Ok(dangling)
}
