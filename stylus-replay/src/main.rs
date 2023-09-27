// Copyright 2022-2023, Offchain Labs, Inc.
// For licensing, see https://github.com/OffchainLabs/stylus-sdk-rs/blob/stylus/licenses/COPYRIGHT.md

use crate::trace::Trace;
use alloy_primitives::{Address, TxHash, B256, U256};
use clap::Parser;
use ethers::{
    providers::{Http, Middleware, Provider},
    types::{GethDebugTracerType, GethDebugTracingOptions, GethTrace},
    utils::__serde_json::Value,
};
use eyre::{bail, Result};

pub use hostio::*;

mod hostio;
mod trace;

#[derive(Parser)]
#[command(author, version, about, name = "replay")]
struct Args {
    #[arg(short, long, default_value = "http://localhost:8545")]
    endpoint: String,
    #[arg(short, long)]
    tx: TxHash,
}

#[tokio::main]
async fn main() -> Result<()> {
    let opts = Args::parse();

    let provider: Provider<Http> = match opts.endpoint.try_into() {
        Ok(provider) => provider,
        Err(error) => bail!("endpoint failure: {error}"),
    };

    let _trace = Trace::new(provider, opts.tx).await?;

    Ok(())
}
