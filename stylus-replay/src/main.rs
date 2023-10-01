// Copyright 2022-2023, Offchain Labs, Inc.
// For licensing, see https://github.com/OffchainLabs/stylus-sdk-rs/blob/stylus/licenses/COPYRIGHT.md

use crate::trace::Trace;
use alloy_primitives::{Address, TxHash};
use clap::Parser;
use ethers::providers::{Http, Provider};
use eyre::{bail, Result};
use std::{os::unix::process::CommandExt, path::PathBuf};

pub use hostio::*;

mod hostio;
mod trace;
mod util;

#[derive(Parser)]
#[command(author, version, about, name = "replay")]
struct Args {
    /// RPC endpoint.
    #[arg(short, long, default_value = "http://localhost:8545")]
    endpoint: String,
    /// Tx to replay.
    #[arg(short, long)]
    tx: TxHash,
    /// Contract to debug. Defaults to the top level contract.
    #[arg(short, long)]
    contract: Option<Address>,
    /// Project path.
    #[arg(short, long, default_value = ".")]
    project: PathBuf,
    /// Whether to use stable Rust. Note that nightly is needed to expand macros.
    #[arg(short, long)]
    stable_rust: bool,
    #[arg(short, long, hide(true))]
    child: bool,
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<()> {
    let opts = Args::parse();

    if !opts.child {
        let mut cmd = util::new_command("rust-gdb");
        cmd.arg("-ex=set breakpoint pending on");
        cmd.arg("-ex=b user_entrypoint");
        cmd.arg("-ex=r");
        cmd.arg("--args");

        for arg in std::env::args() {
            cmd.arg(arg);
        }
        cmd.arg("--child");
        let err = cmd.exec();

        bail!("failed to exec gdb {}", err);
    }

    let provider: Provider<Http> = match opts.endpoint.try_into() {
        Ok(provider) => provider,
        Err(error) => bail!("endpoint failure: {error}"),
    };

    let trace = Trace::new(provider, opts.tx).await?;

    util::build_so(&opts.project, opts.stable_rust)?;
    let so = util::find_so(&opts.project)?;

    // TODO: don't assume the contract is top-level
    let args_len = trace.tx.input.len();

    unsafe {
        *hostio::FRAME.lock() = Some(trace.reader());

        type Entrypoint = unsafe extern "C" fn(usize) -> usize;
        let lib = libloading::Library::new(so)?;
        let main: libloading::Symbol<Entrypoint> = lib.get(b"user_entrypoint")?;

        match main(args_len) {
            0 => println!("call completed successfully"),
            1 => println!("call reverted"),
            x => println!("call exited with unknown status code: {x}"),
        }
    }
    Ok(())
}
