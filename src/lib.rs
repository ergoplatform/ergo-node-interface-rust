#![allow(clippy::ptr_arg)]

#[macro_use]
extern crate json;
pub mod local_config;
pub mod node_interface;
mod requests;
pub mod scanning;
pub mod transactions;

pub use local_config::*;
pub use node_interface::NodeInterface;
pub use scanning::Scan;

/// A Base58 encoded String of a Ergo P2PK address.
pub type P2PKAddressString = String;
/// A JSON String
pub type JsonString = String;
/// A Base58 encoded String of a Ergo P2S address.
pub type P2SAddressString = String;
/// The smallest unit of the Erg currency.
pub type NanoErg = u64;
/// A block height of the chain.
pub type BlockHeight = u64;
/// Duration in number of blocks.
pub type BlockDuration = u64;
/// A Base58 encoded String of a Token ID.
pub type TokenID = String;
/// Integer which is provided by the Ergo node to reference a given scan.
pub type ScanID = String;
