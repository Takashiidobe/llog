use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use chrono::Local;

pub const WIKI_DIR: &str = "wiki";
pub const CONCEPTS_DIR: &str = "concepts";
pub const INDEX_FILE: &str = "index.md";
pub const LOG_DIR: &str = "log";

pub fn wiki_root() -> PathBuf {
    PathBuf::from(WIKI_DIR)
}

fn slugify(title: &str) -> String {
    let mut slug = String::new();
    let mut last_was_dash = false;
    for c in title.chars() {
        if c.is_alphanumeric() {
            slug.push(c.to_ascii_lowercase());
            last_was_dash = false;
        } else if !last_was_dash && !slug.is_empty() {
            slug.push('-');
            last_was_dash = true;
        }
    }
    if slug.ends_with('-') {
        slug.pop();
    }
    slug
}

pub fn init() -> Result<PathBuf> {
    let root = wiki_root();
    if root.exists() {
        anyhow::bail!("{} already exists", root.display());
    }

    fs::create_dir_all(root.join(CONCEPTS_DIR))
        .with_context(|| format!("creating {}", root.join(CONCEPTS_DIR).display()))?;
    fs::create_dir_all(root.join(LOG_DIR))
        .with_context(|| format!("creating {}", root.join(LOG_DIR).display()))?;

    let index_path = root.join(INDEX_FILE);
    fs::write(&index_path, "# Index\n\n")
        .with_context(|| format!("creating {}", index_path.display()))?;

    Ok(root)
}

pub fn new_concept(title: &str) -> Result<PathBuf> {
    let concepts_dir = wiki_root().join(CONCEPTS_DIR);
    fs::create_dir_all(&concepts_dir)
        .with_context(|| format!("creating {}", concepts_dir.display()))?;

    let slug = slugify(title);
    if slug.is_empty() {
        anyhow::bail!("title must contain at least one alphanumeric character");
    }
    let path = concepts_dir.join(format!("{slug}.md"));
    if path.exists() {
        anyhow::bail!("{} already exists", path.display());
    }

    let date = Local::now().format("%Y-%m-%d");
    fs::write(&path, format!("# {title}\n\n_created {date}_\n\n"))
        .with_context(|| format!("writing {}", path.display()))?;

    link_from_index(title, &path)?;

    Ok(path)
}

fn link_from_index(title: &str, concept_path: &Path) -> Result<()> {
    let index_path = wiki_root().join(INDEX_FILE);
    if !index_path.exists() {
        fs::write(&index_path, "# Index\n\n")
            .with_context(|| format!("creating {}", index_path.display()))?;
    }

    let rel = concept_path
        .strip_prefix(wiki_root())
        .unwrap_or(concept_path);
    let link = format!("- [{title}]({})\n", rel.display());

    let mut contents = fs::read_to_string(&index_path)
        .with_context(|| format!("reading {}", index_path.display()))?;
    contents.push_str(&link);
    fs::write(&index_path, contents).with_context(|| format!("writing {}", index_path.display()))
}

pub fn append_log(title: &str, entry: &str) -> Result<PathBuf> {
    let log_dir = wiki_root().join(LOG_DIR);
    fs::create_dir_all(&log_dir).with_context(|| format!("creating {}", log_dir.display()))?;

    let now = Local::now();
    let stamp = now.format("%Y-%m-%d-%H-%M");
    let mut path = log_dir.join(format!("{stamp}.md"));
    let mut suffix = 1;
    while path.exists() {
        path = log_dir.join(format!("{stamp}-{suffix}.md"));
        suffix += 1;
    }

    let contents = format!("# {title}\n\n_{}_\n\n{entry}\n", now.format("%Y-%m-%d %H:%M:%S"));
    fs::write(&path, contents).with_context(|| format!("writing {}", path.display()))?;

    Ok(path)
}
