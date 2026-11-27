//! A struct `Scan` is defined here which wraps the concept of UTXO-set
//! scanning in a Rust-based struct interface.

use crate::node_interface::NodeInterface;
pub use crate::node_interface::{NodeError, Result};
use crate::ScanId;
use ergo_lib::ergotree_ir::chain::ergo_box::{BoxId, ErgoBox};
use serde_json::{from_str, json, to_string_pretty, Value};
use std::collections::HashSet;
use std::convert::TryFrom;

const SCAN_BOX_PAGE_LIMIT: usize = 500;

/// Scanning-related endpoints
impl NodeInterface {
    /// Registers a scan with the node and either returns the `scan_id`
    /// or an error
    pub fn register_scan(&self, scan_json: Value) -> Result<ScanId> {
        let endpoint = "/scan/register";
        let body = scan_json.to_string();
        let res = self.send_post_req(endpoint, body);
        let res_json = self.parse_response_to_json(res)?;

        if !res_json["error"].is_null() {
            return Err(NodeError::BadRequest(res_json["error"].to_string()));
        }
        if res_json["scanId"].is_null() {
            return Err(NodeError::FailedParsingNodeResponse(res_json.to_string()));
        }
        let scan_id = res_json["scanId"].to_string().parse::<ScanId>()?;
        Ok(scan_id)
    }

    pub fn deregister_scan(&self, scan_id: ScanId) -> Result<ScanId> {
        let endpoint = "/scan/deregister";
        let body = generate_deregister_scan_json(scan_id)?;
        let res = self.send_post_req(endpoint, body);
        let res_json = self.parse_response_to_json(res)?;

        if !res_json["error"].is_null() {
            return Err(NodeError::BadRequest(res_json["error"].to_string()));
        }
        if res_json["scanId"].is_null() {
            return Err(NodeError::FailedParsingNodeResponse(res_json.to_string()));
        }
        let scan_id = res_json["scanId"].to_string().parse::<ScanId>()?;
        Ok(scan_id)
    }

    /// Using the `scan_id` of a registered scan, acquires unspent boxes which have been found by said scan
    pub fn scan_boxes(&self, scan_id: ScanId) -> Result<Vec<ErgoBox>> {
        let scan_id_short = i16::try_from(scan_id)?;
        let mut box_list = vec![];
        let mut seen_ids: HashSet<BoxId> = HashSet::new();
        let mut offset: usize = 0;

        loop {
            let endpoint = format!(
                "/scan/unspentBoxes/{scan_id_short}?offset={offset}&limit={SCAN_BOX_PAGE_LIMIT}"
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
                    let mess = format!("Box Json: {box_json}\nError: {e:?}");
                    NodeError::FailedParsingBox(mess)
                })?;
                if seen_ids.insert(ergo_box.box_id()) {
                    box_list.push(ergo_box);
                    added_in_page += 1;
                }
            }

            if page_len < SCAN_BOX_PAGE_LIMIT || added_in_page == 0 {
                break;
            }
            offset = offset
                .checked_add(page_len)
                .ok_or_else(|| NodeError::Other("Pagination offset overflow".to_string()))?;
        }
        Ok(box_list)
    }

    /// Using the `scan_id` of a registered scan, manually adds a box to said
    /// scan.
    pub fn add_box_to_scan(&self, scan_id: ScanId, box_id: &String) -> Result<String> {
        let scan_id_short = i16::try_from(scan_id)?;
        let ergo_box = self.box_from_id(box_id)?;
        let endpoint = "/scan/addBox";

        let body = json!({
            "scanIds": [scan_id_short],
            "box": ergo_box,
        });
        let res = self.send_post_req(endpoint, body.to_string());
        let res_json = self.parse_response_to_json(res)?;
        if res_json["error"].is_null() {
            Ok(res_json.to_string())
        } else {
            Err(NodeError::BadRequest(res_json["error"].to_string()))
        }
    }
}

fn generate_deregister_scan_json(scan_id: ScanId) -> Result<String> {
    let scan_id_short = i16::try_from(scan_id)?;
    let body = json!({
        "scanId": scan_id_short,
    });
    to_string_pretty(&body).map_err(|e| NodeError::Other(e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::requests::test_support::{make_test_box, MockResponse, MockServer};

    #[test]
    fn test_generate_deregister_scan_json() {
        let scan_id = ScanId::from(100);
        expect_test::expect![[r#"
            {
              "scanId": 100
            }"#]]
        .assert_eq(&generate_deregister_scan_json(scan_id).unwrap());
    }

    #[test]
    fn test_scan_boxes_paginates_past_500_boxes() {
        let total_boxes = 505usize;
        let all_entries: Vec<Value> = (0..total_boxes)
            .map(|i| {
                let b = make_test_box(1_000_000, i as u16);
                json!({ "box": b })
            })
            .collect();

        let server = MockServer::spawn(2, move |req| {
            assert_eq!(req.method, "GET");
            if req.target == "/scan/unspentBoxes/42?offset=0&limit=500" {
                MockResponse::ok(Value::Array(all_entries[..500].to_vec()).to_string())
            } else if req.target == "/scan/unspentBoxes/42?offset=500&limit=500" {
                MockResponse::ok(Value::Array(all_entries[500..].to_vec()).to_string())
            } else {
                panic!("unexpected target: {}", req.target);
            }
        });

        let boxes = server.node.scan_boxes(ScanId::from(42)).unwrap();
        assert_eq!(boxes.len(), total_boxes);
        let reqs = server.finish();
        assert_eq!(reqs.len(), 2);
    }

    #[test]
    fn test_scan_boxes_rejects_non_2xx_and_out_of_range_scan_id() {
        let server = MockServer::spawn(1, |_| {
            MockResponse::json(
                403,
                "Forbidden",
                r#"{"error":403,"reason":"invalid.api-key","detail":null}"#,
            )
        });

        assert!(matches!(
            server.node.scan_boxes(ScanId::from(i16::MAX as u64 + 1)),
            Err(NodeError::InvalidScanId(_))
        ));
        assert!(matches!(
            server
                .node
                .deregister_scan(ScanId::from(i16::MAX as u64 + 1)),
            Err(NodeError::InvalidScanId(_))
        ));
        assert!(matches!(
            server
                .node
                .add_box_to_scan(ScanId::from(i16::MAX as u64 + 1), &"00".repeat(32)),
            Err(NodeError::InvalidScanId(_))
        ));
        assert!(matches!(
            server.node.scan_boxes(ScanId::from(42)),
            Err(NodeError::BadRequest(_))
        ));
        server.finish();
    }

    #[test]
    fn test_add_box_to_scan_sends_nested_box_object() {
        let test_box = make_test_box(1_000_000, 7);
        let box_id: String = test_box.box_id().into();
        let box_json = serde_json::to_string(&test_box).unwrap();
        let expected_box_id = box_id.clone();

        let server = MockServer::spawn(2, move |req| {
            if req.method == "GET" && req.target == format!("/utxo/byId/{expected_box_id}") {
                MockResponse::ok(&box_json)
            } else if req.method == "POST" && req.target == "/scan/addBox" {
                let payload: Value = serde_json::from_str(&req.body).expect("valid JSON body");
                assert_eq!(payload["scanIds"], json!([15]));
                assert!(
                    payload["box"].is_object(),
                    "box field must be a nested JSON object, got: {}",
                    payload["box"]
                );
                assert_eq!(
                    payload["box"]["boxId"],
                    Value::String(expected_box_id.clone())
                );
                MockResponse::ok(format!(r#""{expected_box_id}""#))
            } else {
                panic!("unexpected request: {} {}", req.method, req.target);
            }
        });

        let added = server
            .node
            .add_box_to_scan(ScanId::from(15), &box_id)
            .expect("add_box_to_scan should succeed");
        assert_eq!(added, box_id);
        server.finish();
    }

    #[test]
    fn test_scan_boxes_handles_exact_page_boundary_and_register_scan_errors() {
        let all_entries: Vec<Value> = (0..500)
            .map(|i| {
                let b = make_test_box(1_000_000, i as u16);
                json!({ "box": b })
            })
            .collect();

        let server = MockServer::spawn(3, move |req| {
            if req.target == "/scan/unspentBoxes/7?offset=0&limit=500" {
                MockResponse::ok(Value::Array(all_entries.clone()).to_string())
            } else if req.target == "/scan/unspentBoxes/7?offset=500&limit=500" {
                MockResponse::ok("[]")
            } else if req.target == "/scan/register" {
                MockResponse::ok(r#"{"scanId":40000}"#)
            } else {
                panic!("unexpected target: {}", req.target);
            }
        });

        let boxes = server.node.scan_boxes(ScanId::from(7)).unwrap();
        assert_eq!(boxes.len(), 500);
        assert!(matches!(
            server.node.register_scan(json!({"scanName":"test"})),
            Err(NodeError::InvalidScanId(_))
        ));
        server.finish();
    }
}
