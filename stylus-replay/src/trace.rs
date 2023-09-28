// Copyright 2023, Offchain Labs, Inc.
// For licensing, see https://github.com/OffchainLabs/stylus-sdk-rs/blob/stylus/licenses/COPYRIGHT.md

#![allow(clippy::redundant_closure_call)]

use alloy_primitives::{Address, TxHash, B256, U256};
use ethers::{
    providers::{JsonRpcClient, Middleware, Provider},
    types::{
        GethDebugTracerType, GethDebugTracingOptions, GethTrace, Transaction, TransactionReceipt,
    },
    utils::__serde_json::Value,
};
use eyre::{bail, Result};
use std::{collections::VecDeque, mem};

#[derive(Debug)]
pub struct Trace {
    top_frame: TraceFrame,
    pub receipt: TransactionReceipt,
    pub tx: Transaction,
}

impl Trace {
    pub async fn new<T: JsonRpcClient>(provider: Provider<T>, tx: TxHash) -> Result<Self> {
        let hash = tx.0.into();

        let Some(receipt) = provider.get_transaction_receipt(hash).await? else {
            bail!("failed to get receipt for tx: {}", hash)
        };
        let Some(tx) = provider.get_transaction(hash).await? else {
            bail!("failed to get tx data: {}", hash)
        };

        let query = include_str!("query.js");
        let tracer = GethDebugTracingOptions {
            tracer: Some(GethDebugTracerType::JsTracer(query.to_owned())),
            ..GethDebugTracingOptions::default()
        };
        let GethTrace::Unknown(trace) = provider.debug_trace_transaction(hash, tracer).await?
        else {
            bail!("malformed tracing result")
        };

        println!("{}", trace);

        let to = receipt.to.map(|x| Address::from(x.0));
        let top_frame = TraceFrame::parse_frame(to, trace)?;

        println!("{:#?}", top_frame);

        Ok(Self {
            top_frame,
            receipt,
            tx,
        })
    }

    pub fn reader(self) -> FrameReader {
        FrameReader {
            steps: self.top_frame.steps.clone().into(),
            frame: self.top_frame,
        }
    }
}

#[derive(Clone, Debug)]
pub struct TraceFrame {
    steps: Vec<Hostio>,
    address: Option<Address>,
}

impl TraceFrame {
    fn new(address: Option<Address>) -> Self {
        let steps = vec![];
        Self { steps, address }
    }

    pub fn parse_frame(address: Option<Address>, array: Value) -> Result<TraceFrame> {
        let mut frame = TraceFrame::new(address);

        let Value::Array(array) = array else {
            bail!("not an array: {}", array);
        };

        for step in array {
            let Value::Object(mut keys) = step else {
                bail!("not a valid step: {}", step);
            };

            macro_rules! get_typed {
                ($keys:expr, $ty:ident, $name:expr) => {{
                    let value = match $keys.remove($name) {
                        Some(name) => name,
                        None => bail!("object missing {}: {:?}", $name, $keys),
                    };
                    match value {
                        Value::$ty(string) => string,
                        x => bail!("unexpected type for {}: {}", $name, x),
                    }
                }};
            }
            macro_rules! get_int {
                ($name:expr) => {
                    get_typed!(keys, Number, $name).as_u64().unwrap()
                };
            }

            let name = get_typed!(keys, String, "name");
            let args = get_typed!(keys, Array, "args");
            let outs = get_typed!(keys, Array, "outs");

            let start_ink = get_int!("startInk");
            let end_ink = get_int!("endInk");

            fn to_data(values: &[Value]) -> Result<Box<[u8]>> {
                let mut vec = vec![];
                for value in values {
                    let Value::Number(byte) = value else {
                        bail!("expected a byte but found {value}");
                    };
                    let byte = byte.as_u64().unwrap();
                    if byte > 255 {
                        bail!("expected a byte but found {byte}");
                    };
                    vec.push(byte as u8);
                }
                Ok(vec.into_boxed_slice())
            }

            macro_rules! convert {
                ($name:ident, $ty:ident, $conv:expr) => {
                    fn $name(data: &[Value]) -> Result<$ty> {
                        let data = to_data(data)?;
                        if data.len() != mem::size_of::<$ty>() {
                            bail!("expected {}: {}", stringify!($ty), hex::encode(data));
                        }
                        Ok($conv(&data[..]))
                    }
                };
            }

            convert!(to_u8, u8, |x: &[_]| x[0]);
            convert!(to_u16, u16, |x: &[_]| u16::from_be_bytes(
                x.try_into().unwrap()
            ));
            convert!(to_u32, u32, |x: &[_]| u32::from_be_bytes(
                x.try_into().unwrap()
            ));
            convert!(to_u64, u64, |x: &[_]| u64::from_be_bytes(
                x.try_into().unwrap()
            ));
            convert!(to_u256, U256, |x| B256::from_slice(x).into());
            convert!(to_b256, B256, B256::from_slice);
            convert!(to_address, Address, Address::from_slice);

            macro_rules! frame {
                () => {{
                    let mut info = get_typed!(keys, Object, "info");

                    // geth uses the pattern { "0": Number, "1": Number, ... }
                    let address = get_typed!(info, Object, "address");
                    let mut address: Vec<_> = address.into_iter().collect();
                    address.sort_by_key(|x| x.0.parse::<u8>().unwrap());
                    let address: Vec<_> = address.into_iter().map(|x| x.1).collect();

                    let steps = info.remove("steps").unwrap();
                    let to = Some(to_address(&address)?);
                    TraceFrame::parse_frame(to, steps)?
                }};
            }

            use HostioKind::*;
            let kind = match name.as_str() {
                "read_args" => ReadArgs {
                    args: to_data(&outs)?,
                },
                "write_result" => WriteResult {
                    result: to_data(&args)?,
                },
                "msg_value" => MsgValue {
                    value: to_b256(&outs)?,
                },
                "memory_grow" => MemoryGrow {
                    pages: to_u16(&args)?,
                },
                "contract_address" => ContractAddress {
                    address: to_address(&outs)?,
                },
                "call_contract" => CallContract {
                    address: to_address(&args[..20])?,
                    gas: to_u64(&args[20..28])?,
                    value: to_u256(&args[28..60])?,
                    data: to_data(&args[60..])?,
                    outs_len: to_u32(&outs[..4])?,
                    status: to_u8(&outs[4..])?,
                    frame: frame!(),
                },
                "user_entrypoint" | "user_returned" => continue,
                x => todo!("{}", x),
            };

            frame.steps.push(Hostio {
                kind,
                start_ink,
                end_ink,
            });
        }
        Ok(frame)
    }
}

#[derive(Clone, Debug)]
pub struct Hostio {
    pub kind: HostioKind,
    start_ink: u64,
    end_ink: u64,
}

#[derive(Clone, Debug)]
pub enum HostioKind {
    ReadArgs {
        args: Box<[u8]>,
    },
    WriteResult {
        result: Box<[u8]>,
    },
    MsgValue {
        value: B256,
    },
    MemoryGrow {
        pages: u16,
    },
    ContractAddress {
        address: Address,
    },
    CallContract {
        address: Address,
        data: Box<[u8]>,
        gas: u64,
        value: U256,
        outs_len: u32,
        status: u8,
        frame: TraceFrame,
    },
    UserEntrypoint,
    UserReturned,
}

impl HostioKind {
    fn name(&self) -> &'static str {
        use HostioKind as H;
        match self {
            H::ReadArgs { .. } => "read_args",
            H::WriteResult { .. } => "write_result",
            H::MsgValue { .. } => "msg_value",
            H::MemoryGrow { .. } => "memory_grow",
            H::ContractAddress { .. } => "contract_address",
            H::CallContract { .. } => "call_contract",
            H::UserEntrypoint => "user_entrypoint",
            H::UserReturned => "user_returned",
        }
    }
}

#[derive(Debug)]
pub struct FrameReader {
    frame: TraceFrame,
    steps: VecDeque<Hostio>,
}

impl FrameReader {
    fn next(&mut self) -> Result<Hostio> {
        match self.steps.pop_front() {
            Some(item) => Ok(item),
            None => bail!("No next hostio"),
        }
    }

    pub fn next_hostio(&mut self, expected: &'static str) -> Hostio {
        // TODO: the stable compiler's borrow checker can't see that self.next() is bound to
        // the same lifetime, but when it can, refactor this loop.
        loop {
            let hostio = self.next().unwrap();
            println!("Expect: {expected} {hostio:?}");

            if hostio.kind.name() == expected {
                return hostio;
            }
            match hostio.kind.name() {
                "memory_grow" | "user_entrypoint" => continue,
                _ => panic!("incorrect hostio:\nexpected {expected}\nfound {hostio:?}"),
            }
        }
    }
}
