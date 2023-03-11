# cargo-files

[![Latest version](https://img.shields.io/crates/v/cargo-files)](https://crates.io/crates/cargo-files)
[![crates.io downloads](https://img.shields.io/crates/d/cargo-files)](https://crates.io/crates/cargo-files)
[![Build Status](https://img.shields.io/github/actions/workflow/status/dcchut/cargo-files/ci.yml?branch=master)](https://github.com/dcchut/cargo-files/actions)
![Apache/MIT2.0 License](https://img.shields.io/crates/l/cargo-files)

A tool to list all source files in a cargo crate.

## Motivation

While I was writing [cargo-derivefmt](https://github.com/dcchut/cargo-derivefmt) I found myself
wishing for a simple way to get the source files in a cargo crate.  I wasn't able to find
an existing crate which did this, so I wrote this one.

This library is still a work-in-progress.  There are likely many issues and unsupported
situations.

## CLI

For end users, we provide a CLI which lists all source files in a crate.

### Installation

`cargo install` (crates.io)
```shell
cargo install cargo-files --locked
```

`cargo install` (master)
```shell
cargo install --git https://github.com/dcchut/cargo-files --locked
```

### Usage

```shell
cargo files
```

## Developers

The `cargo-files-core` crate contains the logic underlying `cargo-files`, and can
be reused by other applications that care about source files.  At the moment the API
is extremely simplistic, but any improvement suggestions are welcome!

### Minimal example

```rust
use cargo_files_core::{get_targets, get_target_files, Error};

fn main() -> Result<(), Error> {
    // Each target (e.g. bin/lib) in your workspace will contribute a target.
    let targets = get_targets(None)?;
    for target in targets {
        // Get all the files related to a specific target.
        let files = get_target_files(&target)?;
        for file in files {
            println!("{}", file.display());
        }
    }
    
    Ok(())
}
```
