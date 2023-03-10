use std::collections::HashSet;
use std::path::Path;
use cargo_files_core::{get_projects, get_target_files};

fn main() {
    let axum = Path::new("C:/dev/derivefmt/Cargo.toml");

    let projects = get_projects(Some(axum)).unwrap();
    for target in projects {
        dbg!(&target);
        println!("parsing {}", target.path.display());
        let files = get_target_files(&target).unwrap();

        if target.kind == "custom-build" {
            assert_eq!(files.len(), 1);
        } else {
            let mut found_files = HashSet::new();
            let parent = target.path.parent().unwrap().join("**").join("*.rs");
            for p in glob::glob(&parent.to_string_lossy()).unwrap() {
                let p = p.unwrap();
                found_files.insert(p);
            }
            assert_eq!(files, found_files);
        }
    }
}
