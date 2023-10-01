// Copyright 2023, Offchain Labs, Inc.
// For licensing, see https://github.com/OffchainLabs/stylus-sdk-rs/blob/stylus/licenses/COPYRIGHT.md

use eyre::{bail, eyre, Result};
use std::{
    ffi::OsStr,
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

fn new_command<S: AsRef<OsStr>>(program: S) -> Command {
    let mut command = Command::new(program);
    command.stdout(Stdio::inherit()).stderr(Stdio::inherit());
    command
}

pub fn build_so(path: &Path) -> Result<()> {
    new_command("cargo")
        .current_dir(path)
        .arg("build")
        .arg("--lib")
        .arg("--target")
        .arg(rustc_host::from_cli()?)
        .output()?;
    Ok(())
}

pub fn find_so(project: &Path) -> Result<PathBuf> {
    let triple = rustc_host::from_cli()?;
    let so_dir = project.join(format!("target/{triple}/debug/"));
    let so_dir = std::fs::read_dir(&so_dir)
        .map_err(|e| eyre!("failed to open {}: {e}", so_dir.to_string_lossy()))?
        .filter_map(|r| r.ok())
        .map(|r| r.path())
        .filter(|r| r.is_file());

    let mut file: Option<PathBuf> = None;
    for entry in so_dir {
        let Some(ext) = entry.file_name() else {
            continue;
        };
        if ext.to_string_lossy().contains(".so") {
            if let Some(other) = file {
                let other = other.file_name().unwrap().to_string_lossy();
                bail!(
                    "more than one .so found: {other} and {}",
                    ext.to_string_lossy()
                );
            }
            file = Some(entry);
        }
    }
    let Some(file) = file else {
        bail!("failed to find .so");
    };
    Ok(file)
}
