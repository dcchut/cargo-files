// Inspired by  https://github.com/rust-lang/rustfmt
// #![deny(warnings)]

pub mod parser;

use std::cmp::Ordering;
use std::collections::{BTreeSet, HashSet};
use std::hash::{Hash, Hasher};
use std::io::{self};
use std::path::{Path, PathBuf};
use cargo_metadata::Edition;
use thiserror::Error;
use crate::parser::extract_crate_files;

#[derive(Debug, Error)]
pub enum Error {
    #[error("the manifest-path must be a path to a Cargo.toml file")]
    ManifestNotCargoToml,
    #[error("no targets were found")]
    NoTargets,
    #[error("there was an error reading Cargo.toml: {0}")]
    ManifestError(#[from] io::Error),
    #[error("there was an error parsing a source file: {0}")]
    ParseError(#[from] syn::Error),
    #[error("could not find module")]
    ModuleNotFound,
    #[error("source file must have parent")]
    NoParent,
    #[error("source file must have a stem")]
    NoStem,
}


/// Get all source files for the given target.
pub fn get_target_files(target: &Target) -> Result<HashSet<PathBuf>, Error> {
    let mut acc = HashSet::new();
    extract_crate_files(&target.path, &mut acc)?;
    Ok(acc)
}

/// Get all projects within the given cargo workspace.
pub fn get_projects(manifest_path: Option<&Path>) -> Result<BTreeSet<Target>, Error> {
    if let Some(specified_manifest_path) = manifest_path {
        if !specified_manifest_path.ends_with("Cargo.toml") {
            return Err(Error::ManifestNotCargoToml);
        }
        get_targets(Some(specified_manifest_path))
    } else {
        get_targets(None)
    }
}

/// Target uses a `path` field for equality and hashing.
#[derive(Debug)]
pub struct Target {
    /// A path to the main source file of the target.
    pub path: PathBuf,
    /// A kind of target (e.g., lib, bin, example, ...).
    pub kind: String,
    /// Rust edition for this target.
    pub edition: Edition,
}

impl Target {
    pub fn from_target(target: &cargo_metadata::Target) -> Self {
        let path = PathBuf::from(&target.src_path);
        let canonicalized = dunce::canonicalize(&path).unwrap_or(path);

        Target {
            path: canonicalized,
            kind: target.kind[0].clone(),
            edition: target.edition.clone(),
        }
    }
}

impl PartialEq for Target {
    fn eq(&self, other: &Target) -> bool {
        self.path == other.path
    }
}

impl PartialOrd for Target {
    fn partial_cmp(&self, other: &Target) -> Option<Ordering> {
        Some(self.path.cmp(&other.path))
    }
}

impl Ord for Target {
    fn cmp(&self, other: &Target) -> Ordering {
        self.path.cmp(&other.path)
    }
}

impl Eq for Target {}

impl Hash for Target {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.path.hash(state);
    }
}

/// Return a list of the main source files.
fn get_targets(
    manifest_path: Option<&Path>,
) -> Result<BTreeSet<Target>, Error> {
    let mut targets = BTreeSet::new();
    get_targets_recursive(manifest_path, &mut targets, &mut BTreeSet::new())?;

    if targets.is_empty() {
        Err(Error::NoTargets)
    } else {
        Ok(targets)
    }
}


fn get_targets_recursive(
    manifest_path: Option<&Path>,
    targets: &mut BTreeSet<Target>,
    visited: &mut BTreeSet<String>,
) -> Result<(), Error> {
    let metadata = get_cargo_metadata(manifest_path)?;
    for package in &metadata.packages {
        add_targets(&package.targets, targets);

        // Look for local dependencies using information available since cargo v1.51
        for dependency in &package.dependencies {
            if dependency.path.is_none() || visited.contains(&dependency.name) {
                continue;
            }

            let manifest_path = PathBuf::from(dependency.path.as_ref().unwrap()).join("Cargo.toml");
            if manifest_path.exists()
                && !metadata
                .packages
                .iter()
                .any(|p| p.manifest_path.eq(&manifest_path))
            {
                visited.insert(dependency.name.to_owned());
                get_targets_recursive(Some(&manifest_path), targets, visited)?;
            }
        }
    }

    Ok(())
}


fn add_targets(target_paths: &[cargo_metadata::Target], targets: &mut BTreeSet<Target>) {
    for target in target_paths {
        targets.insert(Target::from_target(target));
    }
}


fn get_cargo_metadata(manifest_path: Option<&Path>) -> Result<cargo_metadata::Metadata, io::Error> {
    let mut cmd = cargo_metadata::MetadataCommand::new();
    cmd.no_deps();
    if let Some(manifest_path) = manifest_path {
        cmd.manifest_path(manifest_path);
    }
    cmd.other_options(vec![String::from("--offline")]);

    match cmd.exec() {
        Ok(metadata) => Ok(metadata),
        Err(_) => {
            cmd.other_options(vec![]);
            match cmd.exec() {
                Ok(metadata) => Ok(metadata),
                Err(error) => Err(io::Error::new(io::ErrorKind::Other, error.to_string())),
            }
        }
    }
}