// Copyright 2023, Offchain Labs, Inc.
// For licensing, see https://github.com/OffchainLabs/stylus-sdk-rs/blob/stylus/licenses/COPYRIGHT.md

use alloy_primitives::{Address, TxHash, B256, U256};
use ethers::{
    providers::{Http, JsonRpcClient, Middleware, Provider},
    types::{GethDebugTracerType, GethDebugTracingOptions, GethTrace},
    utils::__serde_json::Value,
};
use eyre::{bail, Result};
use std::mem;

#[derive(Debug)]
pub struct Trace {
    values: Vec<()>,
    status: TxStatus,
}

impl Trace {
    pub async fn new<T: JsonRpcClient>(provider: Provider<T>, tx: TxHash) -> Result<Self> {
        let tx = tx.0.into();

        let query = include_str!("query.js");
        let tracer = GethDebugTracingOptions {
            tracer: Some(GethDebugTracerType::JsTracer(query.to_owned())),
            ..GethDebugTracingOptions::default()
        };
        let GethTrace::Unknown(trace) = provider.debug_trace_transaction(tx, tracer).await? else {
            bail!("malformed tracing result")
        };

        println!("{}", trace);
        println!("{:#?}", TraceFrame::parse_frame(Address::ZERO, trace));

        todo!()
    }
}

#[derive(Debug)]
pub enum TxStatus {
    Success,
    Revert,
    OutOfGas,
}

#[derive(Debug)]
pub struct TraceFrame {
    steps: Vec<Hostio>,
    address: Address,
}

impl TraceFrame {
    fn new(address: Address) -> Self {
        let steps = vec![];
        Self { steps, address }
    }

    pub fn parse_frame(address: Address, array: Value) -> Result<TraceFrame> {
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
            convert!(to_b256, B256, |x| B256::from_slice(x));
            convert!(to_address, Address, |x| Address::from_slice(x));

            macro_rules! frame {
                () => {{
                    let mut info = get_typed!(keys, Object, "info");

                    // geth uses the pattern { "0": Number, "1": Number, ... }
                    let address = get_typed!(info, Object, "address");
                    let mut address: Vec<_> = address.into_iter().collect();
                    address.sort_by_key(|x| x.0.parse::<u8>().unwrap());
                    let address: Vec<_> = address.into_iter().map(|x| x.1).collect();

                    let steps = info.remove("steps").unwrap();
                    TraceFrame::parse_frame(to_address(&address)?, steps)?
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

#[derive(Debug)]
pub struct Hostio {
    kind: HostioKind,
    start_ink: u64,
    end_ink: u64,
}

#[derive(Debug)]
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
