use crate::Error;
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use syn::visit::Visit;
use syn::{Expr, ExprLit, ItemMod, Lit, Meta};

#[derive(Default, Debug)]
struct ModVisitor {
    modules: Vec<Module>,
    stack: Vec<PathComponent>,
}

impl<'ast> Visit<'ast> for ModVisitor {
    fn visit_item_mod(&mut self, item: &'ast ItemMod) {
        // Parse any #[path = "bla.rs"] declaration.
        let mut path = None;
        for attr in &item.attrs {
            let Meta::NameValue(meta) = &attr.meta else {
                continue;
            };

            let Some(attr_ident) = attr.path().get_ident() else {
                continue;
            };
            if attr_ident != "path" {
                continue;
            }

            let Expr::Lit(ExprLit {
                lit: Lit::Str(lit), ..
            }) = &meta.value
            else {
                continue;
            };
            path = Some(lit.value());
            break;
        }

        self.stack.push(PathComponent {
            name: item.ident.to_string(),
            path,
        });

        // AFAIK mod foobar {} blocks don't contribute a file
        if item.content.is_none() {
            self.modules.push(Module {
                parts: self.stack.clone(),
            });
        }

        syn::visit::visit_item_mod(self, item);
        self.stack.pop().expect("should be balanced");
    }
}

#[derive(Clone, Debug)]
struct PathComponent {
    name: String,

    /// An optional path override, set using the #[path = "..."] attribute.
    path: Option<String>,
}

#[derive(Debug)]
struct Module {
    /// The collection of path components that make up this module definition.
    ///
    /// The source code:
    ///
    /// #[path = "a"]
    /// mod a_mod {
    ///   #[path = "b.rs"]
    ///   mod b_mod;
    /// }
    ///
    /// would give rise to a single Module, having two parts.
    parts: Vec<PathComponent>,
}

impl Module {
    /// Return the source file corresponding to this module.
    fn resolve(self, root_path: &Path, source_file_path: &Path) -> Result<PathBuf, Error> {
        assert!(!self.parts.is_empty());

        let source_file_directory = source_file_path
            .parent()
            .ok_or(Error::NoParent)?
            .to_path_buf();

        let mut base_resolution_path = resolve_base_resolution_path(root_path, source_file_path)?;
        let (final_part, head) = self.parts.split_last().unwrap();

        // Handle parent module paths
        for (i, component) in head.iter().enumerate() {
            if let Some(path) = component.path.as_deref() {
                assert!(!path.ends_with(".rs")); // should be a _folder_
                if i == 0 {
                    // Special-case: A top-level module definition having a path attribute
                    // is always resolved relative to the source file.
                    base_resolution_path = source_file_directory.join(path);
                } else {
                    base_resolution_path.push(path);
                }
            } else {
                base_resolution_path.push(&component.name);
            }
        }

        // Handle the actual module definition path
        if let Some(path) = final_part.path.as_deref() {
            if head.is_empty() {
                // There are no parent modules, so this is actually a top-level module definition.
                base_resolution_path = source_file_directory.join(path);
            } else {
                base_resolution_path.push(path);
            }
            return if base_resolution_path.exists() {
                Ok(base_resolution_path)
            } else {
                Err(Error::ModuleNotFound)
            };
        }

        // Look for a new-style module {name}.rs
        base_resolution_path.push(format!("{}.rs", final_part.name));
        if base_resolution_path.exists() {
            return Ok(base_resolution_path);
        }

        // Look for an old-style module {name}/mod.rs
        base_resolution_path.pop();
        base_resolution_path.extend([&final_part.name, "mod.rs"]);
        if base_resolution_path.exists() {
            return Ok(base_resolution_path);
        }

        Err(Error::ModuleNotFound)
    }
}

fn resolve_base_resolution_path(
    root_path: &Path,
    source_file_path: &Path,
) -> Result<PathBuf, Error> {
    let base_name = source_file_path.file_stem().ok_or(Error::NoStem)?;
    let is_mod_rs = (root_path == source_file_path) || base_name == "mod";

    let mut source_file_directory = source_file_path
        .parent()
        .ok_or(Error::NoParent)?
        .to_path_buf();

    // If this is a mod.rs-like file, then paths are resolved relative to the source file's
    // parent directory. If it isn't, then we need to resolve from one level deeper.
    if !is_mod_rs {
        source_file_directory.push(base_name);
    }

    Ok(source_file_directory)
}

pub fn extract_crate_files(
    root_path: &Path,
    path: &Path,
    acc: &mut HashSet<PathBuf>,
) -> Result<(), Error> {
    acc.insert(path.to_path_buf());
    let source = fs::read_to_string(path).map_err(|e| Error::FileError(path.to_path_buf(), e))?;

    // Extract all the mod definitions in the given file
    let file = syn::parse_file(&source)?;
    let mut visitor = ModVisitor::default();
    visitor.visit_file(&file);

    for module in visitor.modules {
        let module_path = module.resolve(root_path, path)?;
        let canonical_module_path = dunce::canonicalize(&module_path).unwrap_or(module_path);
        extract_crate_files(root_path, &canonical_module_path, acc)?;
        acc.insert(canonical_module_path);
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_unique_module_parts(source: &str, parts: Vec<(&str, Option<&str>)>) {
        let file = syn::parse_file(source).unwrap();
        let mut visitor = ModVisitor::default();
        visitor.visit_file(&file);

        assert_eq!(visitor.modules.len(), 1);

        let module = visitor.modules.pop().unwrap();
        assert_eq!(module.parts.len(), parts.len());

        for (path_component, (expected_name, expected_path)) in module.parts.into_iter().zip(parts)
        {
            assert_eq!(path_component.name, expected_name);
            assert_eq!(path_component.path, expected_path.map(String::from));
        }
    }

    #[test]
    fn test_path_attribute_parsing() {
        let source = r#"
        #[path = "apple.rs"]
        mod banana;
        "#;

        assert_unique_module_parts(source, vec![("banana", Some("apple.rs"))]);
    }

    #[test]
    fn test_nested_mod_parsing() {
        let source = r#"
        mod a {
            mod b {
                mod c {
                    mod d;
                }
            }
        }
        "#;

        assert_unique_module_parts(
            source,
            vec![("a", None), ("b", None), ("c", None), ("d", None)],
        );
    }

    #[test]
    fn test_nested_mod_parsing_with_path_attribute() {
        let source = r#"
        mod a {
            mod b {
                #[path = "putty.rs"]
                mod c;
            }
        }
        "#;

        assert_unique_module_parts(
            source,
            vec![("a", None), ("b", None), ("c", Some("putty.rs"))],
        );
    }

    #[test]
    fn test_docstring_on_module_ignored() {
        // Regression test for #7
        let source = r#"
        ///
        mod intern;
        "#;

        assert_unique_module_parts(source, vec![("intern", None)]);
    }

    #[test]
    fn test_path_and_nested_path() {
        let source = r#"
        #[path = "abc"]
        mod thread {
            #[path = "tls.rs"]
            mod data;
        }
        "#;

        assert_unique_module_parts(
            source,
            vec![("thread", Some("abc")), ("data", Some("tls.rs"))],
        );
    }
}
