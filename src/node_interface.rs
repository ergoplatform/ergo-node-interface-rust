/// The `NodeInterface` struct is defined which allows for interacting with an
/// Ergo Node via Rust.
use crate::{BlockHeight, NanoErg, P2PKAddressString, P2SAddressString};
use ergo_lib::ergotree_ir::chain::ergo_box::ErgoBox;
use serde_json::from_str;
use thiserror::Error;

pub type Result<T> = std::result::Result<T, NodeError>;

#[derive(Error, Debug)]
pub enum NodeError {
    #[error("The configured node is unreachable. Please ensure your config is correctly filled out and the node is running.")]
    NodeUnreachable,
    #[error("Failed reading response from node: {0}")]
    FailedParsingNodeResponse(String),
    #[error("Failed parsing JSON box from node: {0}")]
    FailedParsingBox(String),
    #[error("No Boxes Were Found.")]
    NoBoxesFound,
    #[error("An insufficient number of Ergs were found.")]
    InsufficientErgsBalance(),
    #[error("Failed registering UTXO-set scan with the node: {0}")]
    FailedRegisteringScan(String),
    #[error("The node rejected the request you provided.\nNode Response: {0}")]
    BadRequest(String),
    #[error("The node wallet has no addresses.")]
    NoAddressesInWallet,
    #[error("The node is still syncing.")]
    NodeSyncing,
    #[error("Error while processing Node Interface Config Yaml: {0}")]
    YamlError(String),
    #[error("{0}")]
    Other(String),
    #[error("Failed parsing wallet status from node: {0}")]
    FailedParsingWalletStatus(String),
}

/// The `NodeInterface` struct which holds the relevant Ergo node data
/// and has methods implemented to interact with the node.
#[derive(Debug, Clone)]
pub struct NodeInterface {
    pub api_key: String,
    pub ip: String,
    pub port: String,
}

pub fn is_mainnet_address(address: &str) -> bool {
    address.starts_with('9')
}

pub fn is_testnet_address(address: &str) -> bool {
    address.starts_with('3')
}

impl NodeInterface {
    /// Create a new `NodeInterface` using details about the Node
    pub fn new(api_key: &str, ip: &str, port: &str) -> NodeInterface {
        NodeInterface {
            api_key: api_key.to_string(),
            ip: ip.to_string(),
            port: port.to_string(),
        }
    }

    /// Returns `http://ip:port` using `ip` and `port` from self
    pub fn node_url(&self) -> String {
        "http://".to_string() + &self.ip + ":" + &self.port
    }

    /// Get all addresses from the node wallet
    pub fn wallet_addresses(&self) -> Result<Vec<P2PKAddressString>> {
        let endpoint = "/wallet/addresses";
        let res = self.send_get_req(endpoint)?;

        let mut addresses: Vec<String> = vec![];
        for segment in res
            .text()
            .expect("Failed to get addresses from wallet.")
            .split('\"')
        {
            let seg = segment.trim();
            if is_mainnet_address(seg) || is_testnet_address(seg) {
                addresses.push(seg.to_string());
            }
        }
        if addresses.is_empty() {
            return Err(NodeError::NoAddressesInWallet);
        }
        Ok(addresses)
    }

    /// A CLI interactive interface for prompting a user to select an address
    pub fn select_wallet_address(&self) -> Result<P2PKAddressString> {
        let address_list = self.wallet_addresses()?;
        if address_list.len() == 1 {
            return Ok(address_list[0].clone());
        }

        let mut n = 0;
        for address in &address_list {
            n += 1;
            println!("{}. {}", n, address);
        }
        println!("Which address would you like to select?");
        let mut input = String::new();
        if std::io::stdin().read_line(&mut input).is_ok() {
            if let Ok(input_n) = input.trim().parse::<usize>() {
                if input_n > address_list.len() || input_n < 1 {
                    println!("Please select an address within the range.");
                    return self.select_wallet_address();
                }
                return Ok(address_list[input_n - 1].clone());
            }
        }
        self.select_wallet_address()
    }

    /// Acquires unspent boxes from the node wallet
    pub fn unspent_boxes(&self) -> Result<Vec<ErgoBox>> {
        let endpoint = "/wallet/boxes/unspent?minConfirmations=0&minInclusionHeight=0";
        let res = self.send_get_req(endpoint);
        let res_json = self.parse_response_to_json(res)?;

        let mut box_list = vec![];

        for i in 0.. {
            let box_json = &res_json[i]["box"];
            if box_json.is_null() {
                break;
            } else if let Ok(ergo_box) = from_str(&box_json.to_string()) {
                box_list.push(ergo_box);
            }
        }
        Ok(box_list)
    }

    /// Returns unspent boxes from the node wallet ordered from highest to
    /// lowest nanoErgs value.
    pub fn unspent_boxes_sorted(&self) -> Result<Vec<ErgoBox>> {
        let mut boxes = self.unspent_boxes()?;
        boxes.sort_by(|a, b| b.value.as_u64().partial_cmp(a.value.as_u64()).unwrap());

        Ok(boxes)
    }

    /// Returns a sorted list of unspent boxes which cover at least the
    /// provided value `total` of nanoErgs.
    /// Note: This box selection strategy simply uses the largest
    /// value holding boxes from the user's wallet first.
    pub fn unspent_boxes_with_min_total(&self, total: NanoErg) -> Result<Vec<ErgoBox>> {
        self.consume_boxes_until_total(total, &self.unspent_boxes_sorted()?)
    }

    /// Returns a list of unspent boxes which cover at least the
    /// provided value `total` of nanoErgs.
    /// Note: This box selection strategy simply uses the oldest unspent
    /// boxes from the user's full node wallet first.
    pub fn unspent_boxes_with_min_total_by_age(&self, total: NanoErg) -> Result<Vec<ErgoBox>> {
        self.consume_boxes_until_total(total, &self.unspent_boxes()?)
    }

    /// Given a `Vec<ErgoBox>`, consume each ErgoBox into a new list until
    /// the `total` is reached. If there are an insufficient number of
    /// nanoErgs in the provided `boxes` then it returns an error.
    fn consume_boxes_until_total(&self, total: NanoErg, boxes: &[ErgoBox]) -> Result<Vec<ErgoBox>> {
        let mut count = 0;
        let mut filtered_boxes = vec![];
        for b in boxes {
            if count >= total {
                break;
            } else {
                count += b.value.as_u64();
                filtered_boxes.push(b.clone());
            }
        }
        if count < total {
            return Err(NodeError::InsufficientErgsBalance());
        }
        Ok(filtered_boxes)
    }

    /// Acquires the unspent box with the highest value of Ergs inside
    /// from the wallet
    pub fn highest_value_unspent_box(&self) -> Result<ErgoBox> {
        let boxes = self.unspent_boxes()?;

        // Find the highest value amount held in a single box in the wallet
        let highest_value = boxes.iter().fold(0, |acc, b| {
            if *b.value.as_u64() > acc {
                *b.value.as_u64()
            } else {
                acc
            }
        });

        for b in boxes {
            if *b.value.as_u64() == highest_value {
                return Ok(b);
            }
        }
        Err(NodeError::NoBoxesFound)
    }

    /// Acquires the unspent box with the highest value of Ergs inside
    /// from the wallet and serializes it
    pub fn serialized_highest_value_unspent_box(&self) -> Result<String> {
        let ergs_box_id: String = self.highest_value_unspent_box()?.box_id().into();
        self.serialized_box_from_id(&ergs_box_id)
    }

    /// Acquires unspent boxes which cover `total` amount of nanoErgs
    /// from the wallet and serializes the boxes
    pub fn serialized_unspent_boxes_with_min_total(&self, total: NanoErg) -> Result<Vec<String>> {
        let boxes = self.unspent_boxes_with_min_total(total)?;
        let mut serialized_boxes = vec![];
        for b in boxes {
            serialized_boxes.push(self.serialized_box_from_id(&b.box_id().into())?);
        }
        Ok(serialized_boxes)
    }

    /// Given a P2S Ergo address, extract the hex-encoded serialized ErgoTree (script)
    pub fn p2s_to_tree(&self, address: &P2SAddressString) -> Result<String> {
        let endpoint = "/script/addressToTree/".to_string() + address;
        let res = self.send_get_req(&endpoint);
        let res_json = self.parse_response_to_json(res)?;

        Ok(res_json["tree"].to_string())
    }

    /// Given a P2S Ergo address, convert it to a hex-encoded Sigma byte array constant
    pub fn p2s_to_bytes(&self, address: &P2SAddressString) -> Result<String> {
        let endpoint = "/script/addressToBytes/".to_string() + address;
        let res = self.send_get_req(&endpoint);
        let res_json = self.parse_response_to_json(res)?;

        Ok(res_json["bytes"].to_string())
    }

    /// Given an Ergo P2PK Address, convert it to a raw hex-encoded EC point
    pub fn p2pk_to_raw(&self, address: &P2PKAddressString) -> Result<String> {
        let endpoint = "/utils/addressToRaw/".to_string() + address;
        let res = self.send_get_req(&endpoint);
        let res_json = self.parse_response_to_json(res)?;

        Ok(res_json["raw"].to_string())
    }

    /// Given an Ergo P2PK Address, convert it to a raw hex-encoded EC point
    /// and prepend the type bytes so it is encoded and ready
    /// to be used in a register.
    pub fn p2pk_to_raw_for_register(&self, address: &P2PKAddressString) -> Result<String> {
        let add = self.p2pk_to_raw(address)?;
        Ok("07".to_string() + &add)
    }

    /// Given a raw hex-encoded EC point, convert it to a P2PK address
    pub fn raw_to_p2pk(&self, raw: &str) -> Result<P2PKAddressString> {
        let endpoint = "/utils/rawToAddress/".to_string() + raw;
        let res = self.send_get_req(&endpoint);
        let res_json = self.parse_response_to_json(res)?;

        Ok(res_json["address"].to_string())
    }

    /// Given a raw hex-encoded EC point from a register (thus with type encoded characters in front),
    /// convert it to a P2PK address
    pub fn raw_from_register_to_p2pk(&self, typed_raw: &str) -> Result<P2PKAddressString> {
        self.raw_to_p2pk(&typed_raw[2..].to_string())
    }

    /// Given a `Vec<ErgoBox>` return the given boxes (which must be part of the UTXO-set) as
    /// a vec of serialized strings in Base16 encoding
    pub fn serialize_boxes(&self, b: &[ErgoBox]) -> Result<Vec<String>> {
        Ok(b.iter()
            .map(|b| {
                self.serialized_box_from_id(&b.box_id().into())
                    .unwrap_or_else(|_| "".to_string())
            })
            .collect())
    }

    /// Given an `ErgoBox` return the given box (which must be part of the UTXO-set) as
    /// a serialized string in Base16 encoding
    pub fn serialize_box(&self, b: &ErgoBox) -> Result<String> {
        self.serialized_box_from_id(&b.box_id().into())
    }

    /// Given a box id return the given box (which must be part of the
    /// UTXO-set) as a serialized string in Base16 encoding
    pub fn serialized_box_from_id(&self, box_id: &String) -> Result<String> {
        let endpoint = "/utxo/byIdBinary/".to_string() + box_id;
        let res = self.send_get_req(&endpoint);
        let res_json = self.parse_response_to_json(res)?;

        Ok(res_json["bytes"].to_string())
    }

    /// Given a box id return the given box (which must be part of the
    /// UTXO-set) as a serialized string in Base16 encoding
    pub fn box_from_id(&self, box_id: &String) -> Result<ErgoBox> {
        let endpoint = "/utxo/byId/".to_string() + box_id;
        let res = self.send_get_req(&endpoint);
        let res_json = self.parse_response_to_json(res)?;

        if let Ok(ergo_box) = from_str(&res_json.to_string()) {
            Ok(ergo_box)
        } else {
            Err(NodeError::FailedParsingBox(res_json.pretty(2)))
        }
    }

    /// Get the current nanoErgs balance held in the Ergo Node wallet
    pub fn wallet_nano_ergs_balance(&self) -> Result<NanoErg> {
        let endpoint = "/wallet/balances";
        let res = self.send_get_req(endpoint);
        let res_json = self.parse_response_to_json(res)?;

        let balance = res_json["balance"].clone();

        if balance.is_null() {
            Err(NodeError::NodeSyncing)
        } else {
            balance
                .as_u64()
                .ok_or_else(|| NodeError::FailedParsingNodeResponse(res_json.to_string()))
        }
    }

    /// Get the current block height of the blockchain
    pub fn current_block_height(&self) -> Result<BlockHeight> {
        let endpoint = "/info";
        let res = self.send_get_req(endpoint);
        let res_json = self.parse_response_to_json(res)?;

        let height_json = res_json["fullHeight"].clone();

        if height_json.is_null() {
            Err(NodeError::NodeSyncing)
        } else {
            height_json
                .to_string()
                .parse()
                .map_err(|_| NodeError::FailedParsingNodeResponse(res_json.to_string()))
        }
    }

    /// Get wallet status /wallet/status
    pub fn wallet_status(&self) -> Result<WalletStatus> {
        let endpoint = "/wallet/status";
        let res = self.send_get_req(endpoint);
        let res_json = self.parse_response_to_json(res)?;

        if let Ok(wallet_status) = from_str(&res_json.to_string()) {
            Ok(wallet_status)
        } else {
            Err(NodeError::FailedParsingWalletStatus(res_json.pretty(2)))
        }
    }
}

#[derive(serde::Deserialize, serde::Serialize)]
pub struct WalletStatus {
    #[serde(rename = "isInitialized")]
    pub initialized: bool,
    #[serde(rename = "isUnlocked")]
    pub unlocked: bool,
    #[serde(rename = "changeAddress")]
    pub change_address: Option<P2PKAddressString>,
    #[serde(rename = "walletHeight")]
    pub height: BlockHeight,
    #[serde(rename = "error")]
    pub error: Option<String>,
}
