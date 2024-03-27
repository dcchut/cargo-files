fn run_test(krate: &tempfile::TempDir) -> String {
    let crate_root = dunce::canonicalize(krate.path()).unwrap();
    let projects = cargo_files_core::get_targets(Some(&crate_root.join("Cargo.toml"))).unwrap();

    let mut paths = Vec::new();
    for target in projects {
        let files =
            cargo_files_core::get_target_files(&target).expect("failed to get target files");
        for file in files {
            let relative_path = pathdiff::diff_paths(&file, &crate_root).unwrap();
            let components: Vec<_> = relative_path
                .components()
                .map(|c| c.as_os_str().to_string_lossy().into_owned())
                .collect();
            paths.push(components.join("/"));
        }
    }
    paths.sort();
    paths.join("\n")
}

/// Generate a test case which detects which files are present in a crate.
macro_rules! krate {
    ($def:literal) => {
        let krate = ::cargo_files_test::make_crate!($def);
        insta::assert_snapshot!(run_test(&krate));
    };
}

#[test]
fn basic_detection() {
    krate!(
        "
        src:
          - lib.rs [test, whatever]
          - whatever.rs
          - test:
            - mod.rs [cat]
            - cat.rs
            - not_in_the_crate.rs
    "
    );
}

#[test]
fn new_module_layout() {
    krate!(
        "
        src:
          - lib.rs [scooby]
          - scooby.rs [apple, banana]
          - scooby:
            - apple.rs
            - banana.rs
    "
    );
}

#[test]
fn path_attribute() {
    krate!(
        r#"
        src:
          - lib.rs [scooby(apple.rs), banana]
          - apple.rs
          - banana:
            - mod.rs
    "#
    );
}

#[test]
fn nested_module_basic() {
    krate!(
        r#"
        src:
          - lib.rs [inline]; inline [apple]
          - inline:
            - apple.rs
    "#
    );
}

#[test]
fn nested_module_mod_rs() {
    krate!(
        r#"
        src:
          - lib.rs [inline, apple]; inline [inner(other.rs), cat]
          - apple.rs
          - inline:
            - other.rs
            - cat.rs
    "#
    );
}

#[test]
fn nested_module_non_mod_rs() {
    // path attributes in module blocks are resolved differently depending on the kind
    // of source file the path attribute is located in; see previous example
    // c.f. https://doc.rust-lang.org/reference/items/modules.html#the-path-attribute
    krate!(
        r#"
        src:
          - lib.rs [apple]
          - apple.rs [inline]; inline [inner(other.rs), tat]
          - apple:
            - inline:
              - other.rs
              - tat.rs
    "#
    );
}

#[test]
fn module_in_subdir() {
    // Regression test for #7
    krate!(
        r#"
        src:
          - lib.rs [new, subdir2, subdir]; subdir [subdir_module]
          - new.rs [new_sub]
          - new:
            - new_sub.rs
          - subdir:
            - subdir_module.rs
          - subdir2:
            - mod.rs [subdir_module]
            - subdir_module.rs
    "#
    );
}

#[test]
fn module_in_top_level_file() {
    // Based on example in https://github.com/dtolnay/syn/blob/master/src/parse.rs
    krate!(
        r#"
        src:
          - lib.rs [parse]
          - parse.rs [discouraged(discouraged.rs)]
          - discouraged.rs
    "#
    );
}

#[test]
fn difference_between_mod_and_not_mod_1() {
    krate!(
        r#"
        src:
          - lib.rs [a]
          - a.rs [b, c]
          - a:
            - b.rs
            - c:
              - mod.rs
    "#
    );
}

#[test]
fn difference_between_mod_and_not_mod_2() {
    krate!(
        r#"
        src:
          - lib.rs [a]
          - a:
            - mod.rs [b, c]
            - b.rs
            - c:
              - mod.rs
    "#
    );
}

#[test]
fn nested_module_paths() {
    krate!(
        r#"
        src:
          - lib.rs [a]
          - a.rs [b(canned)]; b [data(soup.rs)]
          - canned:
            - soup.rs
    "#
    );
}

#[test]
fn nested_module_paths_in_root() {
    krate!(
        r#"
        src:
          - lib.rs [a(canned)]; a [data(soup.rs)]
          - canned:
            - soup.rs
    "#
    );
}
