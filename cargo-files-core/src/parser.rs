use crate::Error;
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use syn::visit::Visit;
use syn::{Expr, ExprLit, ItemMod, Lit, Meta};

#[derive(Default, Debug)]
struct ModVisitor {
    modules: Vec<Module>,
    stack: Vec<String>,
}

impl<'ast> Visit<'ast> for ModVisitor {
    fn visit_item_mod(&mut self, item: &'ast ItemMod) {
        self.stack.push(item.ident.to_string());

        // Parse any #[path = "bla.rs"] declaration.
        let mut path = None;
        for attr in &item.attrs {
            let Meta::NameValue(meta) = &attr.meta else {
                continue;
            };
            let Expr::Lit(ExprLit {
                lit: Lit::Str(lit), ..
            }) = &meta.value
            else {
                continue;
            };
            path = Some(lit.value());
            break;
        }

        // AFAIK mod foobar {} blocks don't contribute a file
        if item.content.is_none() {
            self.modules.push(Module {
                parts: self.stack.clone(),
                path,
            });
        }

        syn::visit::visit_item_mod(self, item);
        self.stack.pop().expect("should be balanced");
    }
}

#[derive(Debug)]
struct Module {
    parts: Vec<String>,

    /// An optional path override, set using the #[path = "..."] attribute.
    path: Option<String>,
}

impl Module {
    /// Return the source file corresponding to this module.
    fn resolve(self, root_path: &Path, relative_to: &Path) -> Result<PathBuf, Error> {
        // /src/a/b.rs -> b
        let base_name = relative_to.file_stem().ok_or(Error::NoStem)?;

        let is_mod_rs = (root_path == relative_to) || base_name == "mod";

        // /src/a/b.rs -> /src/a/
        let mut base_path = relative_to.parent().ok_or(Error::NoParent)?.to_path_buf();

        // for a non-mod-rs file e.g. /src/a.rs, modules are taken relative to /src/a/
        if !is_mod_rs {
            base_path.push(base_name);
        }

        let (last, parts) = self
            .parts
            .split_last()
            .expect("module must have at least one part");

        for part in parts {
            base_path.push(part);
        }

        // Handling for #[path = "..."] attribute.
        // https://doc.rust-lang.org/reference/items/modules.html#the-path-attribute
        if let Some(path) = self.path.as_ref() {
            base_path.push(path);
            return if base_path.exists() {
                Ok(base_path)
            } else {
                Err(Error::ModuleNotFound)
            };
        }

        base_path.push(format!("{last}.rs"));
        if base_path.exists() {
            return Ok(base_path);
        }

        base_path.pop();
        base_path.extend([last, "mod.rs"]);
        if base_path.exists() {
            Ok(base_path)
        } else {
            Err(Error::ModuleNotFound)
        }
    }
}

pub fn extract_crate_files(
    root_path: &Path,
    path: &Path,
    acc: &mut HashSet<PathBuf>,
) -> Result<(), Error> {
    acc.insert(path.to_path_buf());
    let source = fs::read_to_string(path)?;

    // Extract all the mod definitions in the given file
    let file = syn::parse_file(&source)?;
    let mut visitor = ModVisitor::default();
    visitor.visit_file(&file);

    for module in visitor.modules {
        let module_path = module.resolve(root_path, path)?;
        extract_crate_files(root_path, &module_path, acc)?;
        acc.insert(module_path);
    }

    Ok(())
}
