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

use salt_spray::{find_manifest, find_repo_root};

static CLIPPY_ENV_ARGS: &str = "--env-args=";
static CLIPPY_FILE_IDENTIFICATION: Lazy<Regex> = Lazy::new(|| {
    // The filename ends up in capture group #1
    Regex::new(r"-->\s+([^:]+)").unwrap()
});
static ENV_VAR_REFERENCE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(\$[A-Z_]+)(?:\W|$)").unwrap()
});

fn resolve_env_vars(s: &str) -> String {
    let mut result = s.to_string();
    if let Some(rr) = find_repo_root() {
        result = result.replace("$REPO_ROOT", &rr.to_string_lossy());
    }

    while let Some(captures) = ENV_VAR_REFERENCE.captures(&result) {
        // unwrap here is safe since the capture group is mandatory
        let first_variable_name = captures.get(1).unwrap().as_str();
        if let Ok(val) = env::var(&first_variable_name[1..]) {
            result = result.replace(first_variable_name, &val);
        } else {
            eprintln!("Unrecognized environment variable: {}", first_variable_name);
            process::exit(-1);
        }
    }
    result
}

/// Parses `args` and loads them into the pending command.
fn load_env_args(cmd: &mut Command, args: &Option<String>) {
    if let Some(args) = args {
        for assignment in args.split(';') {
            if let Some((name, val)) = assignment.split_once('=') {
                let val = resolve_env_vars(val);
                cmd.env(name, val);
            }
        }
    }
}

/// Runs Clippy on a crate, but only outputs lints for files in the given set.
fn lint_crate(cargo_toml: &str, files: &BTreeSet<String>, args: &Option<String>) -> i32 {
    let mut result = 0;
    let mut cmd = Command::new("cargo");
    load_env_args(&mut cmd, args);
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
                    // } else {
                    //     eprintln!("None of the files ended with {}", project_relative_filename);
                    }
                // } else {
                //     eprintln!("Bad\n{}", found_lint);
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

    let mut clippy_env_args = None;

    // Clippy can only operate on whole crates at a time, so rather than lint
    // each crate for each file within it, we group the file names first and
    // only run clippy once for each crate.
    let mut files_by_crate: HashMap<String, BTreeSet<String>> = HashMap::new();
    for mut arg in args {
        if arg.starts_with(CLIPPY_ENV_ARGS) {
            clippy_env_args = Some(arg.split_off(CLIPPY_ENV_ARGS.len()));
        } else if let Some(manifest_path) = find_manifest(&arg) {
            let manifest_path = manifest_path.to_string_lossy().to_string();
            let files = files_by_crate.entry(manifest_path).or_default();
            files.insert(arg);
        }
    }

    // pre-commit looks for either changes to files or our return code to indicate
    // that the commit should not be allowed.  Since we can't automatically fix
    // issues, we instead exit with the violation count.
    let mut violation_count = 0;
    for (cargo_toml, files) in files_by_crate.iter() {
        violation_count += lint_crate(cargo_toml, files, &clippy_env_args);
    }
    process::exit(violation_count);
}
