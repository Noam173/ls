#[cfg(feature = "dhat-heap")]
#[global_allocator]
static ALLOC: dhat::Alloc = dhat::Alloc;

const MB: u64 = 1_048_576;
const KB: u64 = 1_024;

use anyhow::{Context, Result};
use chrono::{DateTime, Local};
use clap::Parser;
use colored::Colorize;
use jwalk::WalkDir;
use std::collections::HashMap;
use std::env::current_dir;
use std::fs::canonicalize;
use std::io::{BufWriter, StdoutLock, Write};
use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::path::{Path, PathBuf};

#[derive(Debug, Parser, Clone)]
pub struct Args {
    #[clap(short('a'))]
    all: bool,
    #[clap(short('l'))]
    long: bool,
    #[clap(long("max"), default_value_t = 1)]
    max_depth: usize,
    #[clap(long("min"), default_value_t = 1)]
    min_depth: usize,
    pub paths: Vec<String>,
}

// Cache resolved names so we only do the uid/gid -> name syscall once per id.
struct NameCache {
    users: HashMap<u32, String>,
    groups: HashMap<u32, String>,
}

impl NameCache {
    fn new() -> Self {
        Self {
            users: HashMap::new(),
            groups: HashMap::new(),
        }
    }

    fn user(&mut self, uid: u32) -> String {
        self.users
            .entry(uid)
            .or_insert_with(|| {
                users::get_user_by_uid(uid)
                    .map(|u| u.name().to_string_lossy().to_string())
                    .unwrap_or_else(|| uid.to_string())
            })
            .clone()
    }

    fn group(&mut self, gid: u32) -> String {
        self.groups
            .entry(gid)
            .or_insert_with(|| {
                users::get_group_by_gid(gid)
                    .map(|g| g.name().to_string_lossy().to_string())
                    .unwrap_or_else(|| gid.to_string())
            })
            .clone()
    }
}

pub fn main() -> Result<()> {
    #[cfg(feature = "dhat-heap")]
    let _profiler = dhat::Profiler::new_heap();

    let args = Args::parse();
    let stdout = std::io::stdout();
    let mut out = BufWriter::new(stdout.lock());
    let mut cache = NameCache::new();

    if args.long {
        writeln!(
            out,
            "{} {} {} {} {} {}",
            "Permissions".underline(),
            "Size".underline(),
            "User".underline(),
            "Group".underline(),
            "Date Modified".underline(),
            "Name".underline()
        )?;
    }

    for path in paths(args.paths)? {
        let path_str = path.to_str().with_context(|| "coudnt convert")?;
        writeln!(out, "{}", path_str.underline().bold())?;

        let walk = if args.all {
            WalkDir::new(path).skip_hidden(false)
        } else {
            WalkDir::new(path)
        }
        .max_depth(args.max_depth)
        .min_depth(args.min_depth)
        .sort(true);

        for entry in walk {
            iter_path(&mut out, &mut cache, entry?.path().as_path(), args.long)?;
        }

        writeln!(out)?;
    }

    out.flush()?;
    Ok(())
}

fn paths(paths: Vec<String>) -> Result<Vec<PathBuf>> {
    let mut paths: Vec<PathBuf> = if paths.is_empty() {
        vec![current_dir()?]
    } else {
        paths
            .into_iter()
            .map(canonicalize)
            .collect::<std::io::Result<Vec<PathBuf>>>()?
    };
    paths.sort();
    paths.dedup();
    Ok(paths)
}

fn iter_path(
    out: &mut BufWriter<StdoutLock>,
    cache: &mut NameCache,
    p: &Path,
    long: bool,
) -> anyhow::Result<()> {
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
            "csv" | "parquet" => name = name.green(),
            _ => (),
        }
        if mode & 0o111 != 0 {
            name = name.green().bold();
        }
    }

    if !long {
        write!(out, "{} ", name)?;
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

    let username = cache.user(meta.uid());
    let groupname = cache.group(meta.gid());

    let modified: DateTime<Local> = meta.modified()?.into();
    let date_str = modified.format("%e %b %H:%M").to_string();

    writeln!(
        out,
        "{} {:^5} {} {} {:^14} {}",
        perms,
        size_str,
        username.yellow().bold(),
        groupname.yellow().bold(),
        date_str.blue(),
        name
    )?;

    Ok(())
}
