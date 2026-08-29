mod check;
mod index;
mod wiki;

use std::io::IsTerminal;
use std::process::Command as Process;

use anyhow::{Context, Result};
use chrono::{DateTime, Local, NaiveDate, NaiveDateTime, TimeZone};
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
    New {
        title: String,
        entry: String,
        #[arg(long)]
        date: Option<String>,
    },
    Wiki {
        title: String,
    },
    Search {
        query: String,
        #[arg(short, long, default_value_t = 3)]
        limit: usize,
        #[arg(long)]
        plain: bool,
    },
    Review {
        #[arg(long)]
        mark: Option<String>,
    },
    Check,
}

fn parse_date(s: &str) -> Result<DateTime<Local>> {
    let naive = NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M:%S")
        .or_else(|_| NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M"))
        .or_else(|_| NaiveDate::parse_from_str(s, "%Y-%m-%d").map(|d| d.and_hms_opt(0, 0, 0).unwrap()))
        .with_context(|| {
            format!("could not parse \"{s}\" as a date; expected YYYY-MM-DD, YYYY-MM-DD HH:MM, or YYYY-MM-DD HH:MM:SS")
        })?;

    Local
        .from_local_datetime(&naive)
        .single()
        .with_context(|| format!("\"{s}\" is ambiguous or invalid in the local timezone"))
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
        Command::New { title, entry, date } => {
            let when = date
                .as_deref()
                .map(parse_date)
                .transpose()?
                .unwrap_or_else(Local::now);
            let path = wiki::append_log(&title, &entry, when)?;
            println!("created {}", path.display());
        }
        Command::Wiki { title } => {
            let path = wiki::new_concept(&title)?;
            println!("created {}", path.display());
        }
        Command::Search {
            query,
            limit,
            plain,
        } => {
            let hits = index::search(&query, limit)?;
            if hits.is_empty() {
                println!("no results");
                return Ok(());
            }

            let paths: Vec<_> = hits
                .iter()
                .map(|hit| wiki::wiki_root().join(&hit.path))
                .collect();
            for path in &paths {
                println!("{}", path.display());
            }

            if plain || !std::io::stdout().is_terminal() {
                return Ok(());
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
        Command::Review { mark } => {
            if let Some(mark) = mark {
                wiki::mark_reviewed(std::path::Path::new(&mark))?;
                println!("marked reviewed through {mark}");
                return Ok(());
            }

            let pending = wiki::pending_log_entries()?;
            if pending.is_empty() {
                println!("no unreviewed log entries");
                return Ok(());
            }
            for path in pending {
                println!("{}", path.display());
            }
        }
        Command::Check => {
            let dangling = check::find_dangling_links()?;
            if dangling.is_empty() {
                println!("no dangling links");
                return Ok(());
            }
            for link in &dangling {
                println!("{}: {}", link.file.display(), link.target);
            }
            anyhow::bail!("{} dangling link(s) found", dangling.len());
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{Cli, Command};
    use clap::Parser;

    #[test]
    fn new_parses_as_a_log_command() {
        let cli = Cli::try_parse_from([
            "llog",
            "new",
            "standup",
            "finished the parser change",
            "--date",
            "2026-08-29",
        ])
        .unwrap();

        match cli.command {
            Command::New { title, entry, date } => {
                assert_eq!(title, "standup");
                assert_eq!(entry, "finished the parser change");
                assert_eq!(date.as_deref(), Some("2026-08-29"));
            }
            _ => panic!("new should create a log"),
        }
    }

    #[test]
    fn wiki_parses_as_a_concept_command() {
        let cli = Cli::try_parse_from(["llog", "wiki", "Rust ownership"]).unwrap();

        match cli.command {
            Command::Wiki { title } => assert_eq!(title, "Rust ownership"),
            _ => panic!("wiki should create a concept"),
        }
    }

    #[test]
    fn log_is_not_a_command() {
        assert!(Cli::try_parse_from(["llog", "log", "title", "entry"]).is_err());
    }
}
