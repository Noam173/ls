#[cfg(feature = "dhat-heap")]
#[global_allocator]
static ALLOC: dhat::Alloc = dhat::Alloc;
const MB: u64 = 1_048_576;
const KB: u64 = 1_024;
use anyhow::Context;
use chrono::{DateTime, Local};
use clap::Parser;
use colored::Colorize;
use std::env::current_dir;
use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::path::Path;
use walkdir::{DirEntry, WalkDir};
#[derive(Debug, Parser)]
pub struct Args {
    #[clap(short('a'))]
    all: bool,
    #[clap(short('l'))]
    long: bool,
    #[clap(long("max"), default_value_t = 1)]
    max_depth: usize,
    #[clap(long("min"), default_value_t = 0)]
    min_depth: usize,
    pub paths: Vec<String>,
}

pub fn main() -> anyhow::Result<()> {
    #[cfg(feature = "dhat-heap")]
    let _profiler = dhat::Profiler::new_heap();
    let args = Args::parse();
    let mut paths = if args.paths.is_empty() {
        vec![current_dir()?.to_string_lossy().to_string()]
    } else {
        args.paths
    };
    paths.dedup();
    paths.into_iter().try_for_each(|p| -> anyhow::Result<()> {
        if !args.long {
            println!("{}", p.underline().bold());
        }
        let mut walk = WalkDir::new(p)
            .min_depth(args.min_depth)
            .max_depth(args.max_depth)
            .into_iter();

        if args.long {
            println!(
                "{} {} {} {} {} {}",
                "Permissions".underline(),
                "Size".underline(),
                "User".underline(),
                "Group".underline(),
                "Date Modified".underline(),
                "Name".underline()
            );
        }
        if !args.all {
            walk.filter_entry(|e| !is_hidden(e))
                .try_for_each(|entry| -> anyhow::Result<()> {
                    iter_path(entry?.path(), args.long)?;
                    Ok(())
                })?;
        } else {
            walk.try_for_each(|entry| -> anyhow::Result<()> {
                iter_path(entry?.path(), args.long)?;
                Ok(())
            })?;
        }
        Ok(())
    })?;
    Ok(())
}
fn iter_path(p: &Path, long: bool) -> anyhow::Result<()> {
    let suffix = p.extension().and_then(|f| f.to_str()).unwrap_or("");
    let file_name = p
        .file_name()
        .and_then(|f| f.to_str())
        .with_context(|| "smt went wrong")?;
    let mut name = file_name.white();
    let meta = p.symlink_metadata()?;
    let mode = meta.permissions().mode();
    let is_dir = meta.file_type().is_dir();
    if is_dir {
        name = name.blue();
    } else {
        match suffix {
            "toml" | "py" | "rs" => name = name.yellow(),
            _ => (),
        }
        if mode & 0o111 != 0 {
            name = name.green().bold();
        }
    }

    if !long {
        println!("{}", name);
        return Ok(());
    }

    let d = "-".white().bold();
    let r = "r".yellow().bold();
    let w = "w".red().bold();
    let x = "x".green().bold().underline();
    let kind = if is_dir {
        "d".blue().bold()
    } else {
        ".".white()
    };
    let perms = format!(
        "{}{}{}{}{}{}{}{}{}{}",
        kind,
        if mode & 0o400 != 0 { &r } else { &d },
        if mode & 0o200 != 0 { &w } else { &d },
        if mode & 0o100 != 0 { &x } else { &d },
        if mode & 0o040 != 0 { &r } else { &d },
        if mode & 0o020 != 0 { &w } else { &d },
        if mode & 0o010 != 0 { &x } else { &d },
        if mode & 0o004 != 0 { &r } else { &d },
        if mode & 0o002 != 0 { &w } else { &d },
        if mode & 0o001 != 0 { &x } else { &d },
    );
    let size = meta.size();
    let size_str = if is_dir {
        "-".to_string().green()
    } else if size >= MB {
        format!("{}M", size / MB).bold().green()
    } else if size >= KB {
        format!("{}k", size / KB).bold().green()
    } else {
        size.to_string().green()
    };

    let username = users::get_user_by_uid(meta.uid())
        .map(|u| u.name().to_string_lossy().to_string())
        .unwrap_or_else(|| meta.uid().to_string());
    let groupname = users::get_group_by_gid(meta.gid())
        .map(|g| g.name().to_string_lossy().to_string())
        .unwrap_or_else(|| meta.gid().to_string());
    let modified: DateTime<Local> = meta.modified()?.into();
    let date_str = modified.format("%e %b %H:%M").to_string();

    println!(
        "{} {:^5} {} {} {:^14} {}",
        perms,
        size_str,
        username.yellow().bold(),
        groupname.yellow().bold(),
        date_str.blue(),
        name
    );

    Ok(())
}
fn is_hidden(e: &DirEntry) -> bool {
    e.file_name()
        .to_str()
        .map(|f| f.starts_with("."))
        .unwrap_or(false)
}
