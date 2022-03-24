//! `salt-clip` is a `clippy` plugin for `pre-commit`.
//!
//! It tries to be smarter about how it handles monorepos (and other situations
//! where Cargo workspaces are used) than some other rustfmt pre-commit wrappers.

#![forbid(unsafe_code)]

use std::collections::{BTreeSet, HashMap};
use std::env;
use std::ffi::OsStr; // intentionally unused since this file is its own test data
use std::process::{self, Command, Output};

use once_cell::sync::Lazy;
use regex::Regex;

use salt_spray::find_manifest;

static CLIPPY_FILE_IDENTIFICATION: Lazy<Regex> = Lazy::new(|| {
    // The filename ends up in capture group #1
    Regex::new(r"-->\s+([^:]+)").unwrap()
});

/// Runs Clippy on a crate, but only outputs lints for files in the given set.
fn lint_crate(cargo_toml: &str, files: &BTreeSet<String>) -> i32 {
    let mut result = 0;
    let mut cmd = Command::new("cargo");
    cmd.args(["clippy", "--no-deps", "--quiet", "--manifest-path", cargo_toml]);

    match cmd.output() {
        Ok(Output { stderr, .. }) => {
            let stderr = String::from_utf8_lossy(&stderr);
            for found_lint in stderr.split("\n\n") {
                if let Some(captures) = CLIPPY_FILE_IDENTIFICATION.captures(found_lint) {
                    // unwrap here is safe since the capture group is mandatory
                    let project_relative_filename = captures.get(1).unwrap().as_str();
                    if files.iter().any(|s| s.ends_with(project_relative_filename)) {
                        eprintln!("\n{}", found_lint);
                        result += 1;
                    }
                }
            }
        }
        e => eprintln!("{:?}", e),
    }
    result
}

/// Do the thing
fn main() {
    let mut args = env::args();
    drop(args.next());

    // Clippy can only operate on whole crates at a time, so rather than lint
    // each crate for each file within it, we group the file names first and
    // only run clippy once for each crate.
    let mut files_by_crate: HashMap<String, BTreeSet<String>> = HashMap::new();
    for file in args {
        if let Some(manifest_path) = find_manifest(&file) {
            let manifest_path = manifest_path.to_string_lossy().to_string();
            let files = files_by_crate.entry(manifest_path).or_default();
            files.insert(file);
        }
    }

    let mut violation_count = 0;
    for (cargo_toml, files) in files_by_crate.iter() {
        violation_count += lint_crate(cargo_toml, files);
    }
    process::exit(violation_count);
}
