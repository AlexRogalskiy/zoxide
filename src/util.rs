use std::env;
use std::path::{Component, Path, PathBuf};
use std::process::Command;
use std::time::SystemTime;

use anyhow::{bail, Context, Result};

use crate::db::Epoch;

pub fn canonicalize<P: AsRef<Path>>(path: &P) -> Result<PathBuf> {
    dunce::canonicalize(path).with_context(|| format!("could not resolve path: {}", path.as_ref().display()))
}

pub fn current_dir() -> Result<PathBuf> {
    env::current_dir().context("could not get current directory")
}

pub fn current_time() -> Result<Epoch> {
    let current_time =
        SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).context("system clock set to invalid time")?.as_secs();

    Ok(current_time)
}

/// Constructs a new [`Command`] for launching the program with the name
/// `program`.
///
/// On Windows, CreateProcess implicitly searches the current working directory
/// for the executable, which is a potential security issue. Instead, we resolve
/// the path to the executable and then pass it to CreateProcess.
///
/// On other platforms, this is a no-op.
///
pub fn get_command<P: AsRef<Path>>(program: P) -> Result<Command> {
    let program = program.as_ref();
    if !cfg!(windows) {
        return Ok(Command::new(program));
    }

    let paths = env::var_os("PATH").context("PATH environment variable not set")?;
    for path in env::split_paths(&paths) {
        if path.as_os_str().is_empty() {
            continue;
        }
        let path = path.join(program);
        if path.metadata().map_or(false, |m| !m.is_dir()) {
            return Ok(Command::new(path));
        }
    }

    bail!("executable not found in PATH: {}", program.display());
}

pub fn path_to_str<P: AsRef<Path>>(path: &P) -> Result<&str> {
    let path = path.as_ref();
    path.to_str().with_context(|| format!("invalid unicode in path: {}", path.display()))
}

/// Returns the absolute version of a path. Like [`std::path::Path::canonicalize`], but doesn't
/// resolve symlinks.
pub fn resolve_path<P: AsRef<Path>>(path: &P) -> Result<PathBuf> {
    let path = path.as_ref();
    let base_path;

    let mut components = path.components().peekable();
    let mut stack = Vec::new();

    // initialize root
    if cfg!(windows) {
        use std::path::Prefix;

        fn get_drive_letter<P: AsRef<Path>>(path: P) -> Option<u8> {
            let path = path.as_ref();
            let mut components = path.components();

            match components.next() {
                Some(Component::Prefix(prefix)) => match prefix.kind() {
                    Prefix::Disk(drive_letter) | Prefix::VerbatimDisk(drive_letter) => Some(drive_letter),
                    _ => None,
                },
                _ => None,
            }
        }

        fn get_drive_path(drive_letter: u8) -> PathBuf {
            format!(r"{}:\", drive_letter as char).into()
        }

        fn get_drive_relative(drive_letter: u8) -> Result<PathBuf> {
            let path = current_dir()?;
            if Some(drive_letter) == get_drive_letter(&path) {
                return Ok(path);
            }

            if let Some(path) = env::var_os(format!("={}:", drive_letter as char)) {
                return Ok(path.into());
            }

            let path = get_drive_path(drive_letter);
            Ok(path)
        }

        match components.peek() {
            Some(Component::Prefix(prefix)) => match prefix.kind() {
                Prefix::Disk(drive_letter) => {
                    let disk = components.next().unwrap();
                    if components.peek() == Some(&Component::RootDir) {
                        let root = components.next().unwrap();
                        stack.push(disk);
                        stack.push(root);
                    } else {
                        base_path = get_drive_relative(drive_letter)?;
                        stack.extend(base_path.components());
                    }
                }
                Prefix::VerbatimDisk(drive_letter) => {
                    components.next();
                    if components.peek() == Some(&Component::RootDir) {
                        components.next();
                    }

                    base_path = get_drive_path(drive_letter);
                    stack.extend(base_path.components());
                }
                _ => bail!("invalid path: {}", path.display()),
            },
            Some(Component::RootDir) => {
                components.next();

                let current_dir = env::current_dir()?;
                let drive_letter = get_drive_letter(&current_dir)
                    .with_context(|| format!("could not get drive letter: {}", current_dir.display()))?;
                base_path = get_drive_path(drive_letter);
                stack.extend(base_path.components());
            }
            _ => {
                base_path = current_dir()?;
                stack.extend(base_path.components());
            }
        }
    } else if components.peek() == Some(&Component::RootDir) {
        let root = components.next().unwrap();
        stack.push(root);
    } else {
        base_path = current_dir()?;
        stack.extend(base_path.components());
    }

    for component in components {
        match component {
            Component::Normal(_) => stack.push(component),
            Component::CurDir => (),
            Component::ParentDir => {
                if stack.last() != Some(&Component::RootDir) {
                    stack.pop();
                }
            }
            Component::Prefix(_) | Component::RootDir => unreachable!(),
        }
    }

    Ok(stack.iter().collect())
}

/// Convert a string to lowercase, with a fast path for ASCII strings.
pub fn to_lowercase<S: AsRef<str>>(s: S) -> String {
    let s = s.as_ref();
    if s.is_ascii() {
        s.to_ascii_lowercase()
    } else {
        s.to_lowercase()
    }
}
