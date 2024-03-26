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
pub struct Module {
    parts: Vec<String>,

    /// An optional path override, set using the #[path = "..."] attribute.
    path: Option<String>,
}

impl Module {
    /// Return the source file corresponding to this module.
    fn resolve(self, relative_to: &Path) -> Result<PathBuf, Error> {
        // Handling for #[path = "..."] attribute.
        // https://doc.rust-lang.org/reference/items/modules.html#the-path-attribute
        if let Some(path) = self.path.as_ref() {
            let source_path = relative_to.parent().map(|p| p.join(path));
            return match source_path {
                Some(p) if p.exists() => Ok(p),
                _ => Err(Error::ModuleNotFound((self, relative_to.to_path_buf()))),
            };
        }

        // try find module file according to new module system
        // files modules are relative to src/a.rs -> /src/a/
        let current_with_parts = || {
            let mut path = relative_to.to_path_buf();
            path.set_extension("");
            self.parts.iter().for_each(|p| path.push(p));
            path
        };
        let mut source_path = current_with_parts();
        source_path.set_extension("rs");
        if source_path.exists() {
            return Ok(source_path);
        }

        //
        let parent_with_parts = || {
            relative_to
                .parent()
                .map(|base| PathBuf::from(base))
                .map(|mut path| {
                    self.parts.iter().for_each(|p| path.push(p));
                    path
                })
        };

        let source_path = parent_with_parts().map(|mut p| {
            p.set_extension("rs");
            p
        });

        match source_path {
            Some(p) if p.exists() => return Ok(p),
            _ => {}
        };

        //
        let source_path = parent_with_parts().map(|mut p| {
            p.push("mod.rs");
            p
        });

        match source_path {
            Some(p) if p.exists() => return Ok(p),
            _ => {}
        };

        return Err(Error::ModuleNotFound((self, relative_to.to_path_buf())));
    }
}

pub fn extract_crate_files(path: &Path, acc: &mut HashSet<PathBuf>) -> Result<(), Error> {
    acc.insert(path.to_path_buf());
    let source = fs::read_to_string(path)?;

    // Extract all the mod definitions in the given file
    let file = syn::parse_file(&source)?;
    let mut visitor = ModVisitor::default();
    visitor.visit_file(&file);

    for module in visitor.modules {
        let module_path = module.resolve(path)?;
        extract_crate_files(&module_path, acc)?;
        acc.insert(module_path);
    }

    Ok(())
}
