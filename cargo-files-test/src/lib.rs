//! This crate provides a macro that can be used to test cargo-files-core.

use once_cell::sync::OnceCell;
use proc_macro::TokenStream;
use quote::quote;
use regex::Regex;
use serde::{Deserialize, Deserializer};
use std::collections::HashMap;
use syn::parse;

/// Matches something looking like:
/// - file.rs
/// - file.rs [mod1]
/// - file.rs [mod1, mod2]
/// - file.rs [mod1(path/to/f.rs), mod2]
/// and so on, and so forth.
fn file_regex() -> &'static Regex {
    static FILE_REGEX: OnceCell<Regex> = OnceCell::new();
    FILE_REGEX.get_or_init(|| {
        Regex::new(r"^(?P<name>\w+\.rs)\s*?(\s+\[(?P<modules>(\w+)(\(.*?\))?(\s*,\s*?\w+)*)])?$")
            .expect("failed to compile regex")
    })
}

#[derive(Clone, Debug)]
struct Module {
    name: String,
    path: Option<String>,
}

/// Represents a file description such as mod.rs [cat]
#[derive(Clone, Debug)]
struct File {
    name: String,
    modules: Vec<Module>,
}

impl<'de> Deserialize<'de> for File {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;

        // We have a complicated regex to test if the
        let Some(captures) = file_regex().captures(&value) else {
            return Err(serde::de::Error::custom("value should be in the format `modulename [submodule1, submodule2]`"));
        };

        let name = String::from(captures.name("name").unwrap().as_str());
        let modules = match captures.name("modules") {
            None => Vec::new(),
            Some(modules) => {
                // modules is a comma separated list, with non-significant whitespace
                let modules = modules.as_str();
                modules
                    .split(',')
                    .map(|part| {
                        let (name, path) = if part.contains('(') && part.contains(')') {
                            let (name, path) = part.split_once('(').unwrap();
                            let (path, _) = path.split_once(')').unwrap();

                            (name.trim().to_string(), Some(path.trim().to_string()))
                        } else {
                            (part.trim().to_string(), None)
                        };

                        Module { name, path }
                    })
                    .collect()
            }
        };

        Ok(File { name, modules })
    }
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum DirTreeEntry {
    File(File),
    Directory(DirTree),
}

#[derive(Debug)]
struct Dir {
    path: Vec<String>,
    files: Vec<File>,
}

#[derive(Debug, Deserialize)]
struct DirTree(HashMap<String, Vec<DirTreeEntry>>);

impl DirTree {
    fn dirs(&self, acc: &mut Vec<Dir>) {
        for (folder, entries) in self.0.iter() {
            let mut current_dir = Dir {
                path: vec![folder.clone()],
                files: Vec::new(),
            };

            let mut buf = Vec::new();
            for entry in entries {
                match entry {
                    DirTreeEntry::File(file) => {
                        current_dir.files.push(file.clone());
                    }
                    DirTreeEntry::Directory(dir) => {
                        dir.dirs(&mut buf);
                    }
                }
            }

            for dir in buf.iter_mut() {
                dir.path.insert(0, folder.clone());
            }

            acc.push(current_dir);
            acc.extend(buf);
        }
    }
}

#[proc_macro]
pub fn make_crate(item: TokenStream) -> TokenStream {
    let input: syn::LitStr = parse(item).expect("failed to parse as literal");
    let dir_tree: DirTree = serde_yaml::from_str(&input.value()).expect("failed to parse as yaml");

    let mut acc = Vec::new();
    dir_tree.dirs(&mut acc);

    let folder_creation = acc
        .iter()
        .map(|dir| {
            let path = &dir.path;

            let file_creation = dir.files.iter().map(|file| {
                let path = &file.name;
                let modules = file
                    .modules
                    .iter()
                    .map(|module| {
                        let name = &module.name;
                        if let Some(path) = &module.path {
                            format!("#[path = \"{path}\"]\nmod {name};\n")
                        } else {
                            format!("mod {name};\n")
                        }
                    })
                    .collect::<Vec<_>>()
                    .join("\n");

                quote! {
                    path.push(#path);
                    ::std::fs::write(&path, #modules).expect("failed to write file");
                    path.pop();
                }
            });

            quote! {
                {
                    let suffix: ::std::path::PathBuf = [#(#path),*].iter().collect();
                    let mut path = dir.path().join(suffix);
                    ::std::fs::create_dir(&path).expect("failed to create directory");
                    #(#file_creation)*
                }
            }
        })
        .collect::<Vec<_>>();

    quote!({
        let dir = ::tempfile::tempdir().unwrap();

        ::std::fs::write(dir.path().join("Cargo.toml"), r#"
            [package]
            name = "test-case"
            version = "0.1.0"
            edition = "2021"

            [dependencies]
        "#).expect("failed to write Cargo.toml");
        #(#folder_creation)*

        dir
    })
    .into()
}
