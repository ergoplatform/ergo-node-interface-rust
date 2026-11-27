use crate::node_interface::{
    is_mainnet_address, is_testnet_address, NodeError, NodeInterface, Result,
};
use crate::requests::format_http_error;
use crate::{BlockHeight, NanoErg, P2PKAddressString};
use ergo_lib::ergotree_ir::chain::ergo_box::{BoxId, ErgoBox};
use serde_json::from_str;
use serde_with::serde_as;
use serde_with::NoneAsEmptyString;
use std::collections::HashSet;

const WALLET_BOX_PAGE_LIMIT: usize = 500;

impl NodeInterface {
    /// Get all addresses from the node wallet
    pub fn wallet_addresses(&self) -> Result<Vec<P2PKAddressString>> {
        let endpoint = "/wallet/addresses";
        let res = self.send_get_req(endpoint)?;
        let status = res.status();
        let text = res.text().map_err(|_| {
            NodeError::FailedParsingNodeResponse(
                "Node Response Not Parseable into Text.".to_string(),
            )
        })?;
        if !status.is_success() {
            return Err(NodeError::BadRequest(format_http_error(status, &text)));
        }

        let addresses: Vec<P2PKAddressString> =
            from_str(&text).map_err(|_| NodeError::FailedParsingNodeResponse(text.clone()))?;

        if addresses
            .iter()
            .any(|addr| !is_mainnet_address(addr) && !is_testnet_address(addr))
        {
            return Err(NodeError::FailedParsingNodeResponse(text));
        }

        if addresses.is_empty() {
            return Err(NodeError::NoAddressesInWallet);
        }
        Ok(addresses)
    }

    /// Acquires unspent boxes from the node wallet
    pub fn unspent_boxes(&self) -> Result<Vec<ErgoBox>> {
        let mut box_list = vec![];
        let mut seen_ids: HashSet<BoxId> = HashSet::new();
        let mut offset: usize = 0;

        loop {
            let endpoint = format!(
                "/wallet/boxes/unspent?minConfirmations=0&minInclusionHeight=0&offset={offset}&limit={WALLET_BOX_PAGE_LIMIT}"
            );
            let res = self.send_get_req(&endpoint);
            let res_json = self.parse_response_to_json(res)?;

            if !res_json.is_array() {
                return Err(NodeError::FailedParsingNodeResponse(res_json.to_string()));
            }

            let page_len = res_json.len();
            let mut added_in_page = 0usize;

            for item in res_json.members() {
                let box_json = &item["box"];
                if box_json.is_null() {
                    return Err(NodeError::FailedParsingBox(format!("Box Json: {item}")));
                }
                let ergo_box: ErgoBox = from_str(&box_json.to_string()).map_err(|e| {
                    NodeError::FailedParsingBox(format!("Box Json: {box_json}\nError: {e:?}"))
                })?;
                if seen_ids.insert(ergo_box.box_id()) {
                    box_list.push(ergo_box);
                    added_in_page += 1;
                }
            }

            if page_len < WALLET_BOX_PAGE_LIMIT || added_in_page == 0 {
                break;
            }
            offset = offset
                .checked_add(page_len)
                .ok_or_else(|| NodeError::Other("Pagination offset overflow".to_string()))?;
        }
        Ok(box_list)
    }

    /// Returns unspent boxes from the node wallet ordered from highest to
    /// lowest nanoErgs value.
    pub fn unspent_boxes_sorted(&self) -> Result<Vec<ErgoBox>> {
        let mut boxes = self.unspent_boxes()?;
        boxes.sort_by(|a, b| b.value.as_u64().cmp(a.value.as_u64()));

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
        let mut count: NanoErg = 0;
        let mut filtered_boxes = vec![];
        for b in boxes {
            if count >= total {
                break;
            }
            count = count.checked_add(*b.value.as_u64()).ok_or_else(|| {
                NodeError::Other("NanoErg total overflowed u64 while selecting boxes".to_string())
            })?;
            filtered_boxes.push(b.clone());
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

    /// Unlock wallet
    pub fn wallet_unlock(&self, password: &str) -> Result<bool> {
        let endpoint = "/wallet/unlock";
        let body = object! {
            pass: password,
        };

        let res = self.send_post_req(endpoint, body.to_string())?;

        if res.status().is_success() {
            Ok(true)
        } else {
            let status = res.status();
            let text = res.text().map_err(|_| {
                NodeError::FailedParsingNodeResponse(
                    "Node Response Not Parseable into Text.".to_string(),
                )
            })?;
            Err(NodeError::BadRequest(format_http_error(status, &text)))
        }
    }
}

#[serde_as]
#[derive(serde::Deserialize, serde::Serialize)]
pub struct WalletStatus {
    #[serde(rename = "isInitialized")]
    pub initialized: bool,
    #[serde(rename = "isUnlocked")]
    pub unlocked: bool,
    #[serde(rename = "changeAddress")]
    #[serde_as(as = "NoneAsEmptyString")]
    pub change_address: Option<P2PKAddressString>,
    #[serde(rename = "walletHeight")]
    pub height: BlockHeight,
    #[serde(rename = "error")]
    pub error: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::requests::test_support::{
        make_test_box, MockResponse, MockServer, MAINNET_P2PK, TESTNET_P2PK,
    };
    use ergo_lib::ergotree_ir::chain::ergo_box::box_value::BoxValue;
    use serde_json::{json, Value};

    #[test]
    fn test_parsing_wallet_status_unlocked() {
        let node_response_json_str = r#"{
          "isInitialized": true,
          "isUnlocked": true,
          "changeAddress": "3Wwc4HWrTcYkRycPNhEUSwNNBdqSBuiHy2zFvjMHukccxE77BaX3",
          "walletHeight": 251965,
          "error": ""
        }"#;
        let t: WalletStatus = serde_json::from_str(node_response_json_str).unwrap();
        assert_eq!(t.height, 251965);
    }

    #[test]
    fn test_parsing_wallet_status_locked() {
        let node_response_json_str = r#"{
          "isInitialized": true,
          "isUnlocked": false,
          "changeAddress": "",
          "walletHeight": 251965,
          "error": ""
        }"#;
        let t: WalletStatus = serde_json::from_str(node_response_json_str).unwrap();
        assert_eq!(t.change_address, None);
        assert_eq!(t.height, 251965);
    }

    #[test]
    fn test_wallet_addresses_parses_json_array_and_checks_errors() {
        let server = MockServer::spawn(5, {
            let count = std::sync::atomic::AtomicUsize::new(0);
            move |req| {
                assert_eq!(req.target, "/wallet/addresses");
                match count.fetch_add(1, std::sync::atomic::Ordering::Relaxed) {
                    0 => MockResponse::ok(format!(r#"["{MAINNET_P2PK}","{TESTNET_P2PK}"]"#)),
                    1 => MockResponse::ok("[]"),
                    2 => MockResponse::json(
                        403,
                        "Forbidden",
                        r#"{"error":403,"reason":"invalid.api-key","detail":null}"#,
                    ),
                    3 => MockResponse::ok(r#"{"not":"an-array"}"#),
                    _ => MockResponse::ok(r#"["9not-a-valid-address"]"#),
                }
            }
        });

        let addrs = server.node.wallet_addresses().unwrap();
        assert_eq!(
            addrs,
            vec![MAINNET_P2PK.to_string(), TESTNET_P2PK.to_string()]
        );
        assert!(matches!(
            server.node.wallet_addresses(),
            Err(NodeError::NoAddressesInWallet)
        ));
        assert!(matches!(
            server.node.wallet_addresses(),
            Err(NodeError::BadRequest(_))
        ));
        assert!(matches!(
            server.node.wallet_addresses(),
            Err(NodeError::FailedParsingNodeResponse(_))
        ));
        assert!(matches!(
            server.node.wallet_addresses(),
            Err(NodeError::FailedParsingNodeResponse(_))
        ));
        server.finish();
    }

    #[test]
    fn test_unspent_boxes_paginates_past_500_boxes() {
        let total_boxes = 505usize;
        let all_entries: Vec<Value> = (0..total_boxes)
            .map(|i| {
                let b = make_test_box(1_000_000, i as u16);
                json!({ "box": b })
            })
            .collect();

        let server = MockServer::spawn(2, move |req| {
            assert_eq!(req.method, "GET");
            if req.target
                == "/wallet/boxes/unspent?minConfirmations=0&minInclusionHeight=0&offset=0&limit=500"
            {
                MockResponse::ok(Value::Array(all_entries[..500].to_vec()).to_string())
            } else if req.target
                == "/wallet/boxes/unspent?minConfirmations=0&minInclusionHeight=0&offset=500&limit=500"
            {
                MockResponse::ok(Value::Array(all_entries[500..].to_vec()).to_string())
            } else {
                panic!("unexpected target: {}", req.target);
            }
        });

        let boxes = server.node.unspent_boxes().unwrap();
        assert_eq!(boxes.len(), total_boxes);
        let reqs = server.finish();
        assert_eq!(reqs.len(), 2);
    }

    #[test]
    fn test_unspent_boxes_rejects_non_2xx() {
        let server = MockServer::spawn(1, |_| {
            MockResponse::json(
                403,
                "Forbidden",
                r#"{"error":403,"reason":"invalid.api-key","detail":null}"#,
            )
        });

        assert!(matches!(
            server.node.unspent_boxes(),
            Err(NodeError::BadRequest(_))
        ));
        server.finish();
    }

    #[test]
    fn test_consume_boxes_until_total_checked_add_overflow() {
        let node = NodeInterface::from_url_str("k", "http://127.0.0.1:9/").unwrap();
        let boxes = vec![
            make_test_box(BoxValue::MAX_RAW, 0),
            make_test_box(BoxValue::MAX_RAW, 1),
            make_test_box(BoxValue::MAX_RAW, 2),
        ];

        // Normal selection works without overflow.
        let selected = node
            .consume_boxes_until_total(BoxValue::MAX_RAW, &boxes)
            .unwrap();
        assert_eq!(selected.len(), 1);

        // Requesting u64::MAX across three MAX_RAW boxes would overflow u64 on the 3rd addition.
        let err = node
            .consume_boxes_until_total(u64::MAX, &boxes)
            .expect_err("u64 sum overflow must return Err");
        assert!(matches!(err, NodeError::Other(_)));

        // Insufficient balance still returns InsufficientErgsBalance.
        let small = vec![make_test_box(1_000_000, 0)];
        assert!(matches!(
            node.consume_boxes_until_total(2_000_000, &small),
            Err(NodeError::InsufficientErgsBalance())
        ));
    }

    #[test]
    fn test_unspent_boxes_sorted_and_wallet_unlock_errors() {
        let b_low = make_test_box(1_000_000, 0);
        let b_high = make_test_box(5_000_000, 1);
        let b_mid = make_test_box(2_500_000, 2);
        let payload = json!([{"box": b_low}, {"box": b_high}, {"box": b_mid}]).to_string();

        let server = MockServer::spawn(3, move |req| {
            if req.target.starts_with("/wallet/boxes/unspent") {
                MockResponse::ok(&payload)
            } else if req.target == "/wallet/unlock" && req.body.contains("good") {
                MockResponse::ok("")
            } else {
                MockResponse::json(
                    400,
                    "Bad Request",
                    r#"{"error":400,"reason":"bad.request","detail":null}"#,
                )
            }
        });

        let sorted = server.node.unspent_boxes_sorted().unwrap();
        assert_eq!(
            sorted.iter().map(|b| *b.value.as_u64()).collect::<Vec<_>>(),
            vec![5_000_000, 2_500_000, 1_000_000]
        );
        assert!(server.node.wallet_unlock("good").unwrap());
        assert!(matches!(
            server.node.wallet_unlock("bad"),
            Err(NodeError::BadRequest(_))
        ));
        server.finish();
    }
}
