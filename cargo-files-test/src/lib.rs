//! This crate provides a macro that can be used to test cargo-files-core.

use proc_macro::TokenStream;
use quote::quote;
use regex::Regex;
use serde::{Deserialize, Deserializer};
use std::collections::HashMap;
use std::sync::LazyLock;
use syn::parse;

/// Matches something looking like:
/// - file.rs
/// - file.rs [mod1]
/// - file.rs [mod1, mod2]
/// - file.rs [mod1(path/to/f.rs), mod2]
/// - file.rs [mod1, mod2]; mod1 [mod2 mod3]
///
/// and so on, and so forth.
fn file_regex() -> &'static Regex {
    static FILE_REGEX: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"^(?P<name>\w+(\.rs)?)\s*?(\s+\[(?P<modules>(\w+)(\(.*?\))?(\s*,\s*?\w+)*)])?$")
            .expect("failed to compile regex")
    });
    &*FILE_REGEX
}

#[derive(Clone, Debug)]
struct Module {
    name: String,
    path: Option<String>,
    children: Option<Vec<Module>>,
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

        let mut name_to_module = HashMap::new();

        for part in value.split(';') {
            let Some(captures) = file_regex().captures(part.trim()) else {
                return Err(serde::de::Error::custom(
                    "value should be in the format `modulename [submodule1, submodule2]`",
                ));
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

                            Module {
                                name,
                                path,
                                children: None,
                            }
                        })
                        .collect()
                }
            };

            name_to_module.insert(name, modules);
        }

        // There should be a single root module having a .rs extension
        // FIXME: validate
        let root_entry = name_to_module
            .keys()
            .find(|name| name.ends_with(".rs"))
            .cloned()
            .unwrap();
        let mut root_modules = name_to_module.remove(&root_entry).unwrap();

        let module_name_to_index: HashMap<_, _> = root_modules
            .iter()
            .enumerate()
            .map(|(i, x)| (x.name.clone(), i))
            .collect();

        for (name, module) in name_to_module {
            let target_module: &mut Module = &mut root_modules[module_name_to_index[&name]];
            if target_module.children.is_none() {
                target_module.children = Some(Vec::new());
            }
            target_module.children.as_mut().unwrap().extend(module);
        }

        Ok(File {
            name: root_entry,
            modules: root_modules,
        })
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

fn render_module(module: &Module) -> String {
    let name = &module.name;
    let path_attr = module
        .path
        .as_ref()
        .map_or_else(String::new, |path| format!("#[path = \"{path}\"]\n"));

    if let Some(children) = &module.children {
        let child_entries = children.iter().map(render_module).collect::<Vec<_>>();
        format!(
            "{}mod {name} {{\n{}\n}}",
            path_attr,
            child_entries.join("\n")
        )
    } else {
        format!("{}mod {name};", path_attr)
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
                    .map(render_module)
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
