//! The `NodeInterface` struct is defined which allows for interacting with an Ergo Node via Rust.

use crate::requests::format_http_error;
use crate::{BlockHeight, NanoErg, P2PKAddressString, P2SAddressString};
use ergo_lib::chain::ergo_state_context::{ErgoStateContext, Headers};
use ergo_lib::ergo_chain_types::{Header, PreHeader};
use ergo_lib::ergotree_ir::chain::address::{Address, AddressEncoder, NetworkPrefix};
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
    matches!(
        AddressEncoder::unchecked_parse_network_address_from_str(address),
        Ok(na) if na.network() == NetworkPrefix::Mainnet
    )
}

pub fn is_testnet_address(address: &str) -> bool {
    matches!(
        AddressEncoder::unchecked_parse_network_address_from_str(address),
        Ok(na) if na.network() == NetworkPrefix::Testnet
    )
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

        if !res_json.is_array() {
            return Err(NodeError::FailedParsingNodeResponse(res_json.to_string()));
        }

        let mut box_list = vec![];
        for box_json in res_json.members() {
            // This condition is added due to a bug in the node indexer that returns some spent boxes as unspent.
            if box_json["spentTransactionId"].is_null() {
                let ergo_box = from_str(&box_json.to_string()).map_err(|e| {
                    NodeError::FailedParsingBox(format!("Box Json: {box_json}\nError: {e:?}"))
                })?;
                box_list.push(ergo_box);
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

        if !res_json.is_array() {
            return Err(NodeError::FailedParsingNodeResponse(res_json.to_string()));
        }

        let mut box_list = vec![];
        for box_json in res_json.members() {
            // This condition is added due to a bug in the node indexer that returns some spent boxes as unspent.
            if box_json["spentTransactionId"].is_null() {
                let ergo_box = from_str(&box_json.to_string()).map_err(|e| {
                    NodeError::FailedParsingBox(format!("Box Json: {box_json}\nError: {e:?}"))
                })?;
                box_list.push(ergo_box);
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

        res_json["tree"]
            .as_str()
            .map(str::to_string)
            .ok_or_else(|| NodeError::FailedParsingNodeResponse(res_json.to_string()))
    }

    /// Given a P2S Ergo address, convert it to a hex-encoded Sigma byte array constant
    pub fn p2s_to_bytes(&self, address: &P2SAddressString) -> Result<String> {
        let endpoint = "/script/addressToBytes/".to_string() + address;
        let res = self.send_get_req(&endpoint);
        let res_json = self.parse_response_to_json(res)?;

        res_json["bytes"]
            .as_str()
            .map(str::to_string)
            .ok_or_else(|| NodeError::FailedParsingNodeResponse(res_json.to_string()))
    }

    /// Given an Ergo P2PK Address, convert it to a raw hex-encoded EC point
    pub fn p2pk_to_raw(&self, address: &P2PKAddressString) -> Result<String> {
        let endpoint = "/utils/addressToRaw/".to_string() + address;
        let res = self.send_get_req(&endpoint);
        let res_json = self.parse_response_to_json(res)?;

        res_json["raw"]
            .as_str()
            .map(str::to_string)
            .ok_or_else(|| NodeError::FailedParsingNodeResponse(res_json.to_string()))
    }

    /// Given an Ergo P2PK Address, convert it to a raw hex-encoded EC point
    /// and prepend the type bytes so it is encoded and ready
    /// to be used in a register.
    pub fn p2pk_to_raw_for_register(&self, address: &P2PKAddressString) -> Result<String> {
        let parsed = AddressEncoder::unchecked_parse_network_address_from_str(address)
            .map_err(|e| NodeError::BadRequest(format!("Invalid Ergo address `{address}`: {e}")))?;
        if !matches!(parsed.address(), Address::P2Pk(_)) {
            return Err(NodeError::BadRequest(format!(
                "Expected a P2PK Ergo address, got `{address}`"
            )));
        }
        let add = self.p2pk_to_raw(address)?;
        Ok("07".to_string() + &add)
    }

    /// Given a raw hex-encoded EC point, convert it to a P2PK address
    pub fn raw_to_p2pk(&self, raw: &str) -> Result<P2PKAddressString> {
        let endpoint = "/utils/rawToAddress/".to_string() + raw;
        let res = self.send_get_req(&endpoint);
        let res_json = self.parse_response_to_json(res)?;

        res_json["address"]
            .as_str()
            .map(str::to_string)
            .ok_or_else(|| NodeError::FailedParsingNodeResponse(res_json.to_string()))
    }

    /// Given a raw hex-encoded EC point from a register (thus with type encoded characters in front),
    /// convert it to a P2PK address
    pub fn raw_from_register_to_p2pk(&self, typed_raw: &str) -> Result<P2PKAddressString> {
        let raw = typed_raw
            .strip_prefix("07")
            .filter(|s| !s.is_empty())
            .ok_or_else(|| {
                NodeError::BadRequest(format!(
                    "Invalid register-encoded public key `{typed_raw}`: expected `07` prefix followed by hex bytes"
                ))
            })?;
        self.raw_to_p2pk(raw)
    }

    /// Given a `Vec<ErgoBox>` return the given boxes (which must be part of the UTXO-set) as
    /// a vec of serialized strings in Base16 encoding
    pub fn serialize_boxes(&self, b: &[ErgoBox]) -> Result<Vec<String>> {
        b.iter()
            .map(|b| self.serialized_box_from_id(&b.box_id().into()))
            .collect()
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

        res_json["bytes"]
            .as_str()
            .map(str::to_string)
            .ok_or_else(|| NodeError::FailedParsingNodeResponse(res_json.to_string()))
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
                .as_u64()
                .ok_or_else(|| NodeError::FailedParsingNodeResponse(res_json.to_string()))
        }
    }

    /// Get the current state context of the blockchain
    pub fn get_state_context(&self) -> Result<ErgoStateContext> {
        let mut vec_headers = self.get_last_block_headers(10)?;
        vec_headers.reverse();
        let ten_headers: [Header; 10] = vec_headers.try_into().map_err(|v: Vec<Header>| {
            NodeError::FailedParsingNodeResponse(format!(
                "Expected 10 block headers for state context, got {}",
                v.len()
            ))
        })?;
        let headers = Headers::from(ten_headers);
        let pre_header = PreHeader::from(
            headers
                .first()
                .ok_or_else(|| {
                    NodeError::FailedParsingNodeResponse("Missing latest block header".to_string())
                })?
                .clone(),
        );
        let state_context = ErgoStateContext::new(pre_header, headers);

        Ok(state_context)
    }

    /// Get the last `number` of block headers from the blockchain
    pub fn get_last_block_headers(&self, number: u32) -> Result<Vec<Header>> {
        let endpoint = format!("/blocks/lastHeaders/{}", number);
        let res = self.send_get_req(endpoint.as_str());
        let res_json = self.parse_response_to_json(res)?;

        if !res_json.is_array() {
            return Err(NodeError::FailedParsingNodeResponse(res_json.to_string()));
        }

        let mut headers: Vec<Header> = vec![];
        for header_json in res_json.members() {
            let header = from_str(&header_json.to_string()).map_err(|e| {
                NodeError::FailedParsingNodeResponse(format!(
                    "Failed parsing block header ({header_json}): {e}"
                ))
            })?;
            headers.push(header);
        }
        Ok(headers)
    }

    /// Checks if the blockchain indexer is active by querying the node.
    pub fn indexer_status(&self) -> Result<IndexerStatus> {
        let endpoint = "/blockchain/indexedHeight";
        let res = self.send_get_req(endpoint)?;
        let status = res.status();
        let text = res.text().map_err(|_| {
            NodeError::FailedParsingNodeResponse(
                "Node Response Not Parseable into Text.".to_string(),
            )
        })?;

        if !status.is_success() {
            if let Ok(err_json) = json::parse(&text) {
                if err_json["detail"]
                    .as_str()
                    .map(|d| d.contains("Extra indexing is not enabled"))
                    .unwrap_or(false)
                {
                    return Ok(IndexerStatus {
                        is_active: false,
                        is_sync: false,
                    });
                }
            }
            return Err(NodeError::BadRequest(format_http_error(status, &text)));
        }

        let res_json =
            json::parse(&text).map_err(|_| NodeError::FailedParsingNodeResponse(text))?;

        let error = res_json["error"].clone();
        if !error.is_null() {
            return Ok(IndexerStatus {
                is_active: false,
                is_sync: false,
            });
        }

        let full_height = res_json["fullHeight"]
            .as_u64()
            .ok_or_else(|| NodeError::FailedParsingNodeResponse(res_json.to_string()))?;
        let indexed_height = res_json["indexedHeight"]
            .as_u64()
            .ok_or_else(|| NodeError::FailedParsingNodeResponse(res_json.to_string()))?;

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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::requests::test_support::{
        MockResponse, MockServer, MAINNET_P2PK, MAINNET_P2S, MAINNET_P2SH, TESTNET_P2PK,
        TESTNET_P2S, TESTNET_P2SH,
    };

    #[test]
    fn test_address_network_validation() {
        assert!(is_mainnet_address(MAINNET_P2PK));
        assert!(is_mainnet_address(MAINNET_P2S));
        assert!(is_mainnet_address(MAINNET_P2SH));
        assert!(!is_mainnet_address(TESTNET_P2PK));
        assert!(!is_mainnet_address(TESTNET_P2S));
        assert!(!is_mainnet_address(TESTNET_P2SH));
        assert!(!is_mainnet_address("9not-a-valid-address"));
        assert!(!is_mainnet_address(""));

        assert!(is_testnet_address(TESTNET_P2PK));
        assert!(is_testnet_address(TESTNET_P2S));
        assert!(is_testnet_address(TESTNET_P2SH));
        assert!(!is_testnet_address(MAINNET_P2PK));
        assert!(!is_testnet_address(MAINNET_P2S));
        assert!(!is_testnet_address(MAINNET_P2SH));
        assert!(!is_testnet_address("3not-a-valid-address"));
        assert!(!is_testnet_address(""));
    }

    #[test]
    fn test_p2pk_to_raw_for_register_validates_p2pk_and_http_errors() {
        let server = MockServer::spawn(2, |req| {
            if req.target.contains(MAINNET_P2PK) {
                MockResponse::ok(r#"{"raw":"02aabbcc"}"#)
            } else {
                MockResponse::json(
                    404,
                    "Not Found",
                    r#"{"error":404,"reason":"not-found","detail":null}"#,
                )
            }
        });

        assert_eq!(
            server
                .node
                .p2pk_to_raw_for_register(&MAINNET_P2PK.to_string())
                .unwrap(),
            "0702aabbcc"
        );
        // Non-P2PK and malformed addresses are rejected locally without issuing a request.
        assert!(matches!(
            server
                .node
                .p2pk_to_raw_for_register(&MAINNET_P2S.to_string()),
            Err(NodeError::BadRequest(_))
        ));
        assert!(matches!(
            server
                .node
                .p2pk_to_raw_for_register(&MAINNET_P2SH.to_string()),
            Err(NodeError::BadRequest(_))
        ));
        assert!(matches!(
            server
                .node
                .p2pk_to_raw_for_register(&"9not-valid".to_string()),
            Err(NodeError::BadRequest(_))
        ));
        // A valid P2PK address receiving a 404 with null detail must return Err, not Ok("07null").
        assert!(matches!(
            server
                .node
                .p2pk_to_raw_for_register(&TESTNET_P2PK.to_string()),
            Err(NodeError::BadRequest(_))
        ));
        server.finish();
    }

    #[test]
    fn test_string_endpoints_reject_non_2xx_and_missing_fields() {
        let server = MockServer::spawn(4, |req| {
            if req.target.starts_with("/script/addressToTree/") {
                MockResponse::json(
                    400,
                    "Bad Request",
                    r#"{"error":400,"reason":"bad.request","detail":null}"#,
                )
            } else if req.target.starts_with("/script/addressToBytes/") {
                MockResponse::ok(r#"{}"#)
            } else if req.target.starts_with("/utils/rawToAddress/") {
                MockResponse::json(
                    404,
                    "Not Found",
                    r#"{"error":404,"reason":"not-found","detail":null}"#,
                )
            } else {
                MockResponse::ok(r#"{"bytes":null}"#)
            }
        });

        assert!(matches!(
            server.node.p2s_to_tree(&MAINNET_P2S.to_string()),
            Err(NodeError::BadRequest(_))
        ));
        assert!(matches!(
            server.node.p2s_to_bytes(&MAINNET_P2S.to_string()),
            Err(NodeError::FailedParsingNodeResponse(_))
        ));
        assert!(matches!(
            server.node.raw_to_p2pk("deadbeef"),
            Err(NodeError::BadRequest(_))
        ));
        assert!(matches!(
            server.node.serialized_box_from_id(&"00".repeat(32)),
            Err(NodeError::FailedParsingNodeResponse(_))
        ));
        server.finish();
    }

    #[test]
    fn test_raw_from_register_to_p2pk_rejects_short_or_invalid_inputs_without_panic() {
        let server = MockServer::spawn(1, |req| {
            assert_eq!(req.target, "/utils/rawToAddress/02aabb");
            MockResponse::ok(format!(r#"{{"address":"{MAINNET_P2PK}"}}"#))
        });

        for bad in ["", "0", "07", "é", "08cd02aabb"] {
            assert!(
                matches!(
                    server.node.raw_from_register_to_p2pk(bad),
                    Err(NodeError::BadRequest(_))
                ),
                "expected BadRequest for `{}`",
                bad
            );
        }
        assert_eq!(
            server.node.raw_from_register_to_p2pk("0702aabb").unwrap(),
            MAINNET_P2PK
        );
        server.finish();
    }

    #[test]
    fn test_unspent_boxes_by_address_rejects_non_2xx_instead_of_empty_vec() {
        let server = MockServer::spawn(1, |_| {
            MockResponse::json(
                403,
                "Forbidden",
                r#"{"error":403,"reason":"invalid.api-key","detail":null}"#,
            )
        });

        let err = server
            .node
            .unspent_boxes_by_address(&MAINNET_P2PK.to_string(), 0, 10)
            .expect_err("403 must not return Ok(vec![])");
        assert!(matches!(err, NodeError::BadRequest(_)));
        server.finish();
    }

    #[test]
    fn test_get_state_context_returns_error_when_headers_incomplete() {
        let server = MockServer::spawn(1, |req| {
            assert_eq!(req.target, "/blocks/lastHeaders/10");
            MockResponse::ok("[]")
        });

        let err = server
            .node
            .get_state_context()
            .expect_err("empty header list must return Err instead of panicking");
        assert!(matches!(err, NodeError::FailedParsingNodeResponse(_)));
        server.finish();
    }

    #[test]
    fn test_indexer_status_disabled_vs_http_error() {
        let server = MockServer::spawn(3, {
            let count = std::sync::atomic::AtomicUsize::new(0);
            move |_| match count.fetch_add(1, std::sync::atomic::Ordering::Relaxed) {
                0 => MockResponse::json(
                    500,
                    "Internal Server Error",
                    r#"{"error":500,"reason":"internal.error","detail":"Extra indexing is not enabled"}"#,
                ),
                1 => MockResponse::ok(r#"{"indexedHeight":100,"fullHeight":102}"#),
                _ => MockResponse::json(
                    403,
                    "Forbidden",
                    r#"{"error":403,"reason":"invalid.api-key","detail":null}"#,
                ),
            }
        });

        let disabled = server.node.indexer_status().unwrap();
        assert!(!disabled.is_active);
        assert!(!disabled.is_sync);

        let active = server.node.indexer_status().unwrap();
        assert!(active.is_active);
        assert!(active.is_sync);

        assert!(matches!(
            server.node.indexer_status(),
            Err(NodeError::BadRequest(_))
        ));
        server.finish();
    }

    #[test]
    fn test_serialize_boxes_propagates_http_errors() {
        let b = crate::requests::test_support::make_test_box(1_000_000, 0);
        let server = MockServer::spawn(1, |_| {
            MockResponse::json(
                404,
                "Not Found",
                r#"{"error":404,"reason":"not-found","detail":null}"#,
            )
        });

        let err = server
            .node
            .serialize_boxes(&[b])
            .expect_err("404 must propagate as Err");
        assert!(matches!(err, NodeError::BadRequest(_)));
        server.finish();
    }
}
