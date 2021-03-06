//! `salt-spray` is a collection of code quality plugins for `pre-commit`.
//! It tries to be smarter about how it handles monorepos (and other situations
//! where Cargo workspaces are used) than some other rustfmt pre-commit wrappers.
//!
//! Why salt spray?  Simple, salt spray is what you use when you're ready to commit
//! to rust.
//!

#![deny(missing_docs)]
#![forbid(unsafe_code)]

use std::env;
use std::ffi::OsStr;
use std::path::{Path, PathBuf};

/// `find_manifest` starts from a given filename and walks up the directory
/// tree until it finds a Cargo.toml file.  It then returns the path to that
/// manifest (including the "Cargo.toml" filename).
pub fn find_manifest<S: AsRef<OsStr> + ?Sized>(filename: &S) -> Option<PathBuf> {
    let filename = Path::new(filename);
    for parent in filename.ancestors() {
        let cargo = parent.join("Cargo.toml");
        if cargo.exists() {
            return Some(cargo);
        }
    }
    None
}

/// Starting from $PWD, search up until a .git directory is found, and return
/// that as the repo root.
pub fn find_repo_root() -> Option<PathBuf> {
    env::current_dir().or_else(|_| {
        env::var("PWD").map(PathBuf::from)
    }).ok().and_then(|p| {
        for parent in p.ancestors() {
            if parent.join(".git").exists() {
                return Some(parent.to_path_buf());
            }
        }
        None
    })
}
