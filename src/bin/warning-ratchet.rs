//! The Warning Ratchet is a linter that ensures that the number of allowed warnings
//! does not increase.
//!
//! It does that by parsing every file in the change, and counting each warning listed
//! in an #[allow(lint)] block, and then comparing those totals to the previous totals
//! stored in .therug.yaml .  If the totals match the ratchet does nothing.  If the
//! new total of some lint has increased, the ratchet rejects that commit and lets the
//! user know why.  If OTOH the new total has decreased the ratchet clicks and updates
//! .therug.yaml to have fewer warnings swept under it.
//!

// #![allow(unused_must_use, unused_imports)] // Leave these in; this file is its own test data.
#![allow(unused_variables)]

use std::cmp::{max, min};
use std::collections::BTreeMap;
use std::env;
use std::ffi::OsStr;
use std::fs::File;
use std::io::{Read, Write};
use std::path::Path;

use proc_macro2::Span;
use serde::{Deserialize, Serialize};
use syn::{Attribute, Ident, Item};

#[allow(dead_code)]
const SHAMEFILE: &str = ".therug.yaml";

#[allow(dead_code, unsafe_code, unused_mut, unused_imports)]
#[derive(Debug, Default, Serialize, Deserialize)]
struct SupressedLints {
    lints: BTreeMap<String, BTreeMap<String, usize>>,
}

enum Relationship {
    Expected,
    ProperSubset,
    NotASubset,
}

fn read_file<S: AsRef<OsStr>>(filename: S) -> Option<String> {
    File::open(filename.as_ref())
        .map(|mut file| {
            let mut result = String::new();
            file.read_to_string(&mut result).unwrap();
            result
        })
        .ok()
}

#[allow(unused_mut)]
fn look_under_therug() -> SupressedLints {
    // TODO (mrd): this should probably only be a default, test an env var first
    match read_file(SHAMEFILE) {
        Some(contents) => serde_yaml::from_str(&contents).expect(&contents),
        None => Default::default(),
    }
}

fn sweep_under_therug(lints: &SupressedLints) {
    let contents = serde_yaml::to_string(&lints).unwrap();
    // TODO (mrd): this should probably only be a default, test an env var first
    let mut file = File::create(SHAMEFILE).unwrap();
    file.write_all(contents.as_bytes()).unwrap();
}

fn find_supressed_lints<S: AsRef<OsStr>>(filenames: &Vec<S>) -> SupressedLints {
    let mut result = SupressedLints::default();
    for name in filenames {
        if Path::new(&name).extension().map(|e| e == "rs").unwrap_or(false) {
            result.load_suppressed_lints_from(&name.as_ref().to_string_lossy());
        }
    }
    result
}

// #[allow(unsafe_code)]
fn count_lints_in_attrs(
    result: &mut BTreeMap<String, usize>,
    attrs: &Vec<Attribute>,
    item_count: usize,
) {
    let item_count = max(item_count, 1);
    let allow = Ident::new("allow", Span::call_site());

    for attr in attrs {
        if attr.path.get_ident() == Some(&allow) {
            if let Ok(syn::Meta::List(lints)) = attr.parse_meta() {
                for lint in lints.nested {
                    // hooray metaprogramming  :-/
                    if let syn::NestedMeta::Meta(syn::Meta::Path(lint)) = lint {
                        if let Some(lint) = lint.get_ident() {
                            let lint = lint.to_string();
                            if result.contains_key(&lint) {
                                *result.get_mut(&lint).unwrap() += item_count;
                            } else {
                                result.insert(lint, item_count);
                            }
                        }
                    }
                }
            }
        }
    }
}

fn count_lints_in_items(result: &mut BTreeMap<String, usize>, items: &Vec<Item>) {
    for item in items {
        match item {
            Item::Const(c) => count_lints_in_attrs(result, &c.attrs, 1),
            Item::Enum(c) => count_lints_in_attrs(result, &c.attrs, 1),
            Item::ExternCrate(c) => count_lints_in_attrs(result, &c.attrs, 1),
            Item::Fn(c) => count_lints_in_attrs(result, &c.attrs, 1),
            Item::ForeignMod(c) => count_lints_in_attrs(result, &c.attrs, c.items.len()),
            Item::Impl(c) => count_lints_in_attrs(result, &c.attrs, c.items.len()),
            Item::Macro(c) => count_lints_in_attrs(result, &c.attrs, 1),
            Item::Macro2(c) => count_lints_in_attrs(result, &c.attrs, 1),
            Item::Mod(c) => {
                if let Some((_, items)) = &c.content {
                    count_lints_in_attrs(result, &c.attrs, items.len());
                    count_lints_in_items(result, items);
                } else {
                    count_lints_in_attrs(result, &c.attrs, 1);
                }
            }
            Item::Static(c) => count_lints_in_attrs(result, &c.attrs, 1),
            Item::Struct(c) => count_lints_in_attrs(result, &c.attrs, 1),
            Item::Trait(c) => count_lints_in_attrs(result, &c.attrs, 1),
            Item::TraitAlias(c) => count_lints_in_attrs(result, &c.attrs, 1),
            Item::Type(c) => count_lints_in_attrs(result, &c.attrs, 1),
            Item::Union(c) => count_lints_in_attrs(result, &c.attrs, 1),
            Item::Use(c) => count_lints_in_attrs(result, &c.attrs, 1),
            Item::Verbatim(_) => (),
            _ => (),
        }
    }
}

fn count_suppressed_lints(ast: syn::File) -> BTreeMap<String, usize> {
    let mut result = BTreeMap::<String, usize>::default();
    count_lints_in_attrs(&mut result, &ast.attrs, ast.items.len());
    count_lints_in_items(&mut result, &ast.items);
    result
}

fn main() {
    let mut args = env::args();
    drop(args.next());
    let relevant_files: Vec<String> = args.collect();
    let observed_supressed_lints = find_supressed_lints(&relevant_files);
    let mut expected_supressed_lints = look_under_therug();

    match observed_supressed_lints.vis_a_vis(&expected_supressed_lints) {
        Relationship::Expected => (),
        Relationship::ProperSubset => {
            expected_supressed_lints.shrink_around(&observed_supressed_lints, &relevant_files);
            sweep_under_therug(&expected_supressed_lints);
            println!(
                "Thanks for enabling more lints!  Please run `git add {}` and retry your commit.",
                SHAMEFILE
            );
            std::process::exit(2);
        }
        Relationship::NotASubset => {
            // For the most part NotASubset is handled by the eprintln calls below
            if env::var("UPDATE_ANYWAY").map(|v| v == "1").unwrap_or(false) {
                expected_supressed_lints.grow_around(&observed_supressed_lints);
                sweep_under_therug(&expected_supressed_lints);
            }
            std::process::exit(1);
        }
    }
}

#[allow(unsafe_code)]
impl SupressedLints {
    fn vis_a_vis(&self, other: &SupressedLints) -> Relationship {
        let mut result = Relationship::Expected;

        // Is everything in self also in other?
        for (file, lints) in self.lints.iter() {
            if let Some(olints) = other.lints.get(file) {
                for (lint, count) in lints {
                    if let Some(ocount) = olints.get(lint) {
                        if *count < *ocount {
                            result = Relationship::ProperSubset;
                        } else if *count > *ocount {
                            eprintln!("Cannot allow({}) count to increase in {}", lint, file);
                            return Relationship::NotASubset;
                        }
                    } else {
                        eprintln!("Cannot add allow({}) to {}", lint, file);
                        return Relationship::NotASubset;
                    }
                }
            } else if lints.len() > 0 {
                eprintln!("Cannot surpress new lints in {}", file);
                return Relationship::NotASubset;
            }
        }

        // Is there anything in other that is not in self?
        for (ofile, olints) in other.lints.iter() {
            if let Some(lints) = self.lints.get(ofile) {
                for (lint, count) in olints {
                    if !lints.contains_key(lint) && *count > 0 {
                        println!("No longer have {} to worry about in {}.", lint, ofile);
                        result = Relationship::ProperSubset;
                    }
                }
            }
            // No else here because pre-commit chunks the filenames before
            // invoking us, so on the second invocation we expect to have
            // lints that other does not.
        }
        result
    }

    fn shrink_around(&mut self, other: &SupressedLints, examined_files: &Vec<String>) {
        // TODO: remove keys that are now missing?
        for (key, val) in self.lints.iter_mut() {
            if let Some(oval) = other.lints.get(key) {
                for (lint, count) in val.iter_mut() {
                    if let Some(ocount) = oval.get(lint) {
                        *count = min(*count, *ocount);
                    } else {
                        *count = 0;
                    }
                }
            } else if examined_files.contains(key) {
                val.clear();
            }
        }
    }

    fn grow_around(&mut self, other: &SupressedLints) {
        for (okey, oval) in other.lints.iter() {
            if let Some(val) = self.lints.get_mut(okey) {
                for (lint, ocount) in oval {
                    if let Some(count) = val.get_mut(lint) {
                        *count = max(*count, *ocount);
                    } else {
                        val.insert(lint.to_string(), *ocount);
                    }
                }
            } else {
                self.lints.insert(okey.to_string(), oval.clone());
            }
        }
    }

    fn load_suppressed_lints_from(&mut self, filename: &str) {
        if let Some(contents) = read_file(filename) {
            let ast = syn::parse_file(&contents).unwrap();
            self.lints.insert(filename.to_string(), count_suppressed_lints(ast));
        }
    }
}
