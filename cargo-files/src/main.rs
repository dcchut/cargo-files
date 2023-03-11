use cargo_files_core::{get_target_files, get_targets, Error};
use clap::Parser;
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

    let targets = get_targets(args.manifest_path.as_deref())?;
    for target in targets {
        let files = get_target_files(&target)?;
        for file in files {
            println!("{}", file.display());
        }
    }

    Ok(())
}
