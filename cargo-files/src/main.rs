use cargo_files_core::{get_target_files, get_targets, Error};
use clap::Parser;
use std::collections::HashSet;
use std::path::PathBuf;

/// List all files in a cargo crate.
#[derive(Debug, Parser)]
#[command(author, version, about, long_about = None)]
// Cargo passes "files" to cargo-files, so add a hidden argument to capture that.
#[command(
    arg(clap::Arg::new("dummy")
    .value_parser(["files"])
    .required(false)
    .hide(true))
)]
struct Args {
    /// Path to Cargo.toml
    #[arg(long)]
    manifest_path: Option<PathBuf>,
}

fn main() -> Result<(), Error> {
    let args: Args = Args::parse();

    // Note that multiple targets may end up using the same files (e.g. tests);
    // only include each file in the output once.
    let targets = get_targets(args.manifest_path.as_deref())?;
    let mut files = HashSet::new();
    for target in targets {
        files.extend(get_target_files(&target)?);
    }

    let mut files = files.into_iter().collect::<Vec<_>>();
    files.sort();
    for file in files {
        println!("{}", file.display());
    }

    Ok(())
}
