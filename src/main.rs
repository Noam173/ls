const KB: u64 = 1024;
const MB: u64 = KB.pow(2);
use anyhow::{Context, Result};
use chrono::{DateTime, Local};
use clap::{Parser, ValueEnum};
use colored::{Colorize, control};
use core::fmt::NumBuffer;
use jwalk::WalkDir;
use rustc_hash::{FxHashMap, FxHashSet};
use std::env::current_dir;
use std::fs::canonicalize;
use std::io::{BufWriter, Write, stdout};
use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::path::PathBuf;
#[derive(Debug, Clone, ValueEnum)]
enum SetColor {
    Always,
    Never,
}
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
    #[clap(short,long,value_enum,default_value_t = SetColor::Always)]
    color: SetColor,
    paths: Option<Vec<String>>,
}

struct NameCache {
    users: FxHashMap<u32, String>,
    groups: FxHashMap<u32, String>,
}

impl NameCache {
    fn new() -> Self {
        Self {
            users: FxHashMap::default(),
            groups: FxHashMap::default(),
        }
    }

    fn get_user(&mut self, uid: u32) -> String {
        self.users
            .entry(uid)
            .or_insert_with(|| {
                users::get_user_by_uid(uid)
                    .map(|u| u.name().to_string_lossy().to_string())
                    .unwrap_or_else(|| uid.to_string())
            })
            .clone()
    }

    fn get_group(&mut self, gid: u32) -> String {
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
    let args = Args::parse();
    match args.color {
        SetColor::Always => control::set_override(true),
        SetColor::Never => control::set_override(false),
    };
    let mut out = BufWriter::new(stdout().lock());
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
    let paths = paths(args.paths)?;
    for path in paths.iter() {
        if paths.len() > 1 {
            let path_str = path.to_str().with_context(|| "coudnt convert")?;
            writeln!(out, "{}", path_str.underline().bold())?;
        }
        let walk = if args.all {
            WalkDir::new(path).skip_hidden(false)
        } else {
            WalkDir::new(path).skip_hidden(true)
        }
        .max_depth(args.max_depth)
        .min_depth(args.min_depth)
        .sort(true);

        walk.into_iter().try_for_each(|entry| -> Result<()> {
            iter_path(&mut out, &mut cache, entry?.path(), args.long)?;
            Ok(())
        })?;
        out.write_all(b"\n")?;
    }

    out.flush()?;
    Ok(())
}

fn paths(paths: Option<Vec<String>>) -> Result<FxHashSet<PathBuf>> {
    let paths = match paths {
        Some(p) => p
            .into_iter()
            .map(canonicalize)
            .collect::<Result<FxHashSet<_>, _>>()?,
        None => [current_dir()?].into_iter().collect(),
    };

    Ok(paths)
}

fn iter_path(out: &mut impl Write, cache: &mut NameCache, p: PathBuf, long: bool) -> Result<()> {
    let mut buf = NumBuffer::new();
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
            "lock" => name = name.purple(),
            _ => (),
        }
        if mode & 0o111 != 0 {
            name = name.green().bold();
        }
    }

    if !long {
        write!(out, "{}", name)?;
        out.write_all(b" ")?;
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
        "4.0k".green()
    } else if size >= MB {
        format!("{}M", size / MB).bold().green()
    } else if size >= KB {
        format!("{}k", size / KB).bold().green()
    } else {
        size.format_into(&mut buf).green()
    };

    let username = cache.get_user(meta.uid());
    let groupname = cache.get_group(meta.gid());

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
