// Copyright 2022-2023, Offchain Labs, Inc.
// For licensing, see https://github.com/OffchainLabs/stylus-sdk-rs/blob/stylus/licenses/COPYRIGHT.md

use crate::trace::Trace;
use alloy_primitives::{Address, TxHash};
use clap::Parser;
use ethers::providers::{Http, Provider};
use eyre::{bail, Result};
use std::path::PathBuf;

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
}

#[tokio::main]
async fn main() -> Result<()> {
    let opts = Args::parse();

    let provider: Provider<Http> = match opts.endpoint.try_into() {
        Ok(provider) => provider,
        Err(error) => bail!("endpoint failure: {error}"),
    };

    let trace = Trace::new(provider, opts.tx).await?;
    let so = util::find_so(&opts.project)?;

    // TODO: don't assume the contract is top-level
    let args_len = trace.tx.input.len();

    unsafe {
        *hostio::FRAME.lock() = Some(trace.reader());

        type Entrypoint = unsafe extern "C" fn(usize) -> usize;
        let lib = libloading::Library::new(so)?;
        let func: libloading::Symbol<Entrypoint> = lib.get(b"user_entrypoint")?;
        let status = func(args_len);
        println!("contract exited with status: {status}");
    }

    Ok(())
}
