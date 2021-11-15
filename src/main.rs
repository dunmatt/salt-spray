//! `salt-spray` is a rustfmt plugin for `pre-commit`.  It tries to be smarter about
//! how it handles monorepos (and other situations where Cargo workspaces are used)
//! than some other rustfmt pre-commit wrappers.
//!
//! Why salt spray?  Simple, salt spray is what you use when you'er ready to commit
//! to rust.
//!

use std::env;
use std::ffi::OsStr;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

/// Split a given file path into the path of the file's workspace and the relative
/// path from the workspace to the file.
fn split_at_workspace<S: AsRef<OsStr> + ?Sized>(filename: &S) -> Option<(PathBuf, PathBuf)> {
    let mut outermost_cargo_toml = None;
    let filename = Path::new(filename);

    for parent in filename.ancestors() {
        if parent.join("Cargo.lock").exists() || parent.join("target").exists() {
            let relative_path = filename.strip_prefix(parent).unwrap();
            return Some((parent.to_path_buf(), relative_path.to_path_buf()))
        }
        if parent.join("Cargo.toml").exists() {
            outermost_cargo_toml = Some(parent.to_path_buf())
        }
    }
    // only trick is that the lock file and target dir may not exist, so in that
    // case look for Cargo.toml
    outermost_cargo_toml.map(|parent| {
        let relative_path = filename.strip_prefix(&parent).unwrap();
        (parent, relative_path.to_path_buf())
    })
}

/// Format a single file using `cargo fmt`
fn format_file<S: AsRef<OsStr> + ?Sized>(filename: &S) -> std::io::Result<Output> {
    if let Some((workspace, relative_path)) = split_at_workspace(filename) {
        let mut cmd = Command::new("cargo");
        cmd.args(["fmt", "--", "--color", "never", relative_path.to_str().unwrap()]);
        cmd.current_dir(workspace);
        cmd.output()
    } else {
        Err(std::io::Error::new(ErrorKind::NotFound,
            format!("No workspace found for {:?}", filename.as_ref().to_str())))
    }
}

/// Do the thing
fn main() {
    let mut args = env::args();
    drop(args.next());

    for arg in args {
        match format_file(&arg) {
            Ok(Output{status, ..}) if status.code() == Some(0) => {}
            Ok(Output{stderr, ..}) => eprintln!("{}", String::from_utf8_lossy(&stderr)),
            r => eprintln!("{:?}", r),
        }
    }
}
