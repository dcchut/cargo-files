use cargo_files_core::{get_projects, get_target_files};
use clap::Parser;
use std::path::PathBuf;

/// List all files in a cargo crate.
#[derive(Debug, Parser)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Path to Cargo.toml
    #[arg(long)]
    manifest_path: Option<PathBuf>,
}

fn main() {
    let args: Args = Args::parse();

    let projects = get_projects(args.manifest_path.as_deref()).unwrap();
    for target in projects {
        let files = get_target_files(&target).unwrap();
        for file in files {
            println!("{}", file.display());
        }
    }
}
