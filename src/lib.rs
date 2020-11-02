#[macro_use]
extern crate json;
pub mod local_config;
pub mod node_interface;
mod requests;
pub mod scanning;

pub use node_interface::NodeInterface;
pub use scanning::Scan;

type JsonString = String;
