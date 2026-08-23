mod index;
mod wiki;

use std::process::Command as Process;

use anyhow::Result;
use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "llog", about = "A local work-log wiki with search")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    Init,
    New { title: String },
    Log { title: String, entry: String },
    Search {
        query: String,
        #[arg(short, long, default_value_t = 3)]
        limit: usize,
    },
}

fn vim_search_pattern(query: &str) -> String {
    let terms: Vec<String> = query
        .split_whitespace()
        .map(|term| {
            let mut escaped = String::new();
            for c in term.chars() {
                if "\\/.*$^~[]".contains(c) {
                    escaped.push('\\');
                }
                escaped.push(c);
            }
            escaped.replace('\'', "''")
        })
        .collect();
    format!(r"\c\({}\)", terms.join(r"\|"))
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Command::Init => {
            let path = wiki::init()?;
            index::build()?;
            println!("initialized {}", path.display());
        }
        Command::New { title } => {
            let path = wiki::new_concept(&title)?;
            println!("created {}", path.display());
        }
        Command::Log { title, entry } => {
            let path = wiki::append_log(&title, &entry)?;
            println!("created {}", path.display());
        }
        Command::Search { query, limit } => {
            let hits = index::search(&query, limit)?;
            if hits.is_empty() {
                println!("no results");
                return Ok(());
            }

            let paths: Vec<_> = hits.iter().map(|hit| wiki::wiki_root().join(&hit.path)).collect();
            for path in &paths {
                println!("{}", path.display());
            }

            let pattern = vim_search_pattern(&query);
            let status = Process::new("nvim")
                .arg("-c")
                .arg(format!("let @/ = '{pattern}'"))
                .arg("-c")
                .arg("set hlsearch")
                .arg("-c")
                .arg("silent! call search(@/, 'c')")
                .arg("-c")
                .arg("autocmd BufWinEnter * silent! call search(@/, 'c')")
                .args(&paths)
                .status()?;
            if !status.success() {
                anyhow::bail!("nvim exited with {status}");
            }
        }
    }

    Ok(())
}
