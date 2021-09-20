# Ergo Node Interface Library

A Rust library which makes interacting with and using an Ergo Node simple.

This crate uses the great [ergo-lib](https://github.com/ergoplatform/sigma-rust) (formerly sigma-rust) for parsing the `ErgoBox`es from the Ergo Node and other Ergo-related data-types.

Currently supported features include:
1. Core Ergo Node endpoints for writing off-chain dApps.
2. Helper functions on top of the supported endpoints which simplify the dApp developer experience.
3. A higher level interface for UTXO-set scanning.

100% of all Ergo Node endpoints are not supported in the present version, as the current goal is to focus on making the off-chain dApp developer experience as solid as possible. Full endpoint coverage is indeed a goal for the long-term nonetheless.


Modules
========

The below are the currently implemented modules part of the Ergo Node Interface library.

Node Interface
--------------
This module contains the core `NodeInterface` struct which is used to interact with an Ergo Node. All endpoints are implemented as methods on the `NodeInterface` struct.


```rust
let node = NodeInterface::new(api_key, ip, port);
println!("Current height: {}", node.current_block_height());
```

Furthermore a number of helper methods are implemented as well, such as:

```rust
/// A CLI interactive interface for prompting a user to select an address
pub fn select_wallet_address(&self) -> Result<P2PKAddressString>


/// Returns a sorted list of unspent boxes which cover at least the
/// provided value `total` of nanoErgs.
/// Note: This box selection strategy simply uses the largest
/// value holding boxes from the user's wallet first.
pub fn unspent_boxes_with_min_total(&self, total: NanoErg) -> Result<Vec<ErgoBox>>

```

Scanning
---------
This module contains the `Scan` struct which allows a developer to easily work with UTXO-set scans. Each `Scan` is tied to a specific `NodeInterface`, which is inspired from the fact that scans are saved on a per-node basis.

The `Scan` struct provides you with the ability to easily:
1. Register new scans with an Ergo Node.
2. Acquire boxes/serialized boxes from your registered scans.
3. Save/read scan ids to/from a local file.

Example using the scanning interface to register a scan to track an Oracle Pool:

```rust
let oracle_pool_nft_id = "08b59b14e4fdd60e5952314adbaa8b4e00bc0f0b676872a5224d3bf8591074cd".to_string();

let tracking_rule = object! {
        "predicate": "containsAsset",
        "assetId": oracle_pool_nft_id,
};

let scan = Scan::register(
    &"Oracle Pool Box Scan".to_string(),
    tracking_rule,
    node,
).unwrap();

```


Local Config
------------
This module provides a few helper functions to save/read from a local `node-interface.yaml` file which holds the Ergo Node ip/port/api key. This makes it much quicker for a dApp developer to get their dApp running without having to manually implement such logic himself.

Example functions which are available:

```rust
/// Create a new `node-interface.config` with the barebones yaml inside
pub fn create_new_local_config_file() -> Result<()>

/// Opens a local `node-interface.yaml` file and uses the
/// data inside to create a `NodeInterface`
pub fn new_interface_from_local_config() -> Result<NodeInterface> {
```



Documentation
============

Documentation can be accessed via running the following command:

```rust
cargo doc --open
```


Contributing
============
If you find a mistake, want to add a new endpoint, or wish to include a novel feature, please feel free to submit a PR.