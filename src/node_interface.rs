//! The `NodeInterface` struct is defined which allows for interacting with an Ergo Node via Rust.

use crate::{BlockHeight, NanoErg, P2PKAddressString, P2SAddressString};
use ergo_lib::chain::ergo_state_context::{ErgoStateContext, Headers};
use ergo_lib::ergo_chain_types::{Header, PreHeader};
use ergo_lib::ergotree_ir::chain::ergo_box::ErgoBox;
use ergo_lib::ergotree_ir::chain::token::TokenId;
use reqwest::Url;
use serde_json::from_str;
use std::convert::TryInto;
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
    #[error("Failed to parse URL: {0}")]
    InvalidUrl(String),
    #[error("Failed to parse scan ID: {0}")]
    InvalidScanId(String),
}

/// The `NodeInterface` struct which holds the relevant Ergo node data
/// and has methods implemented to interact with the node.
#[derive(Debug, Clone)]
pub struct NodeInterface {
    pub api_key: String,
    pub url: Url,
}

pub fn is_mainnet_address(address: &str) -> bool {
    address.starts_with('9')
}

pub fn is_testnet_address(address: &str) -> bool {
    address.starts_with('3')
}

impl NodeInterface {
    /// Create a new `NodeInterface` using details about the Node
    /// Sets url to `http://ip:port` using `ip` and `port`
    pub fn new(api_key: &str, ip: &str, port: &str) -> Result<Self> {
        let url = Url::parse(("http://".to_string() + ip + ":" + port + "/").as_str())
            .map_err(|e| NodeError::InvalidUrl(e.to_string()))?;
        Ok(NodeInterface {
            api_key: api_key.to_string(),
            url,
        })
    }

    pub fn from_url(api_key: &str, url: Url) -> Self {
        NodeInterface {
            api_key: api_key.to_string(),
            url,
        }
    }

    pub fn from_url_str(api_key: &str, url: &str) -> Result<Self> {
        let url = Url::parse(url).map_err(|e| NodeError::InvalidUrl(e.to_string()))?;
        Ok(NodeInterface {
            api_key: api_key.to_string(),
            url,
        })
    }

    /// Acquires unspent boxes from the blockchain by specific address
    pub fn unspent_boxes_by_address(
        &self,
        address: &P2PKAddressString,
        offset: u64,
        limit: u64,
    ) -> Result<Vec<ErgoBox>> {
        let endpoint = format!(
            "/blockchain/box/unspent/byAddress?offset={}&limit={}",
            offset, limit
        );
        let res = self.send_post_req(endpoint.as_str(), address.clone());
        let res_json = self.parse_response_to_json(res)?;

        let mut box_list = vec![];

        for i in 0.. {
            let box_json = &res_json[i];
            if box_json.is_null() {
                break;
            } else if let Ok(ergo_box) = from_str(&box_json.to_string()) {
                // This condition is added due to a bug in the node indexer that returns some spent boxes as unspent.
                if box_json["spentTransactionId"].is_null() {
                    box_list.push(ergo_box);
                }
            }
        }
        Ok(box_list)
    }

    /// Acquires unspent boxes from the blockchain by specific token_id
    pub fn unspent_boxes_by_token_id(
        &self,
        token_id: &TokenId,
        offset: u64,
        limit: u64,
    ) -> Result<Vec<ErgoBox>> {
        let id: String = (*token_id).into();
        let endpoint = format!(
            "/blockchain/box/unspent/byTokenId/{}?offset={}&limit={}",
            id, offset, limit
        );
        let res = self.send_get_req(endpoint.as_str());
        let res_json = self.parse_response_to_json(res)?;

        let mut box_list = vec![];

        for i in 0.. {
            let box_json = &res_json[i];
            if box_json.is_null() {
                break;
            } else if let Ok(ergo_box) = from_str(&box_json.to_string()) {
                // This condition is added due to a bug in the node indexer that returns some spent boxes as unspent.
                if box_json["spentTransactionId"].is_null() {
                    box_list.push(ergo_box);
                }
            }
        }
        Ok(box_list)
    }

    /// Get the current nanoErgs balance held in the `address`
    pub fn nano_ergs_balance(&self, address: &P2PKAddressString) -> Result<NanoErg> {
        let endpoint = "/blockchain/balance";
        let res = self.send_post_req(endpoint, address.clone());
        let res_json = self.parse_response_to_json(res)?;

        let balance = res_json["confirmed"]["nanoErgs"].clone();

        if balance.is_null() {
            Err(NodeError::NodeSyncing)
        } else {
            balance
                .as_u64()
                .ok_or_else(|| NodeError::FailedParsingNodeResponse(res_json.to_string()))
        }
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
        self.raw_to_p2pk(&typed_raw[2..])
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

    /// Get the current state context of the blockchain
    pub fn get_state_context(&self) -> Result<ErgoStateContext> {
        let mut vec_headers = self.get_last_block_headers(10)?;
        vec_headers.reverse();
        let ten_headers: [Header; 10] = vec_headers.try_into().unwrap();
        let headers = Headers::from(ten_headers);
        let pre_header = PreHeader::from(headers.first().unwrap().clone());
        let state_context = ErgoStateContext::new(pre_header, headers);

        Ok(state_context)
    }

    /// Get the last `number` of block headers from the blockchain
    pub fn get_last_block_headers(&self, number: u32) -> Result<Vec<Header>> {
        let endpoint = format!("/blocks/lastHeaders/{}", number);
        let res = self.send_get_req(endpoint.as_str());
        let res_json = self.parse_response_to_json(res)?;

        let mut headers: Vec<Header> = vec![];

        for i in 0.. {
            let header_json = &res_json[i];
            if header_json.is_null() {
                break;
            } else if let Ok(header) = from_str(&header_json.to_string()) {
                headers.push(header);
            }
        }
        Ok(headers)
    }

    /// Checks if the blockchain indexer is active by querying the node.
    pub fn indexer_status(&self) -> Result<IndexerStatus> {
        let endpoint = "/blockchain/indexedHeight";
        let res = self.send_get_req(endpoint);
        let res_json = self.parse_response_to_json(res)?;

        let error = res_json["error"].clone();
        if !error.is_null() {
            return Ok(IndexerStatus {
                is_active: false,
                is_sync: false,
            });
        }

        let full_height = res_json["fullHeight"]
            .as_u64()
            .ok_or(NodeError::FailedParsingNodeResponse(res_json.to_string()))?;
        let indexed_height = res_json["indexedHeight"]
            .as_u64()
            .ok_or(NodeError::FailedParsingNodeResponse(res_json.to_string()))?;

        let is_sync = full_height.abs_diff(indexed_height) < 10;
        Ok(IndexerStatus {
            is_active: true,
            is_sync,
        })
    }
}

pub struct IndexerStatus {
    pub is_active: bool,
    pub is_sync: bool,
}
