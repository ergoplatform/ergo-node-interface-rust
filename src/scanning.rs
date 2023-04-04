//! A struct `Scan` is defined here which wraps the concept of UTXO-set
//! scanning in a Rust-based struct interface.

use crate::node_interface::NodeInterface;
pub use crate::node_interface::{NodeError, Result};
use crate::ScanId;
use ergo_lib::ergotree_ir::chain::ergo_box::ErgoBox;
use serde_json::{from_str, Value};
use serde_json::{json, to_string_pretty};

/// Scanning-related endpoints
impl NodeInterface {
    /// Registers a scan with the node and either returns the `scan_id`
    /// or an error
    pub fn register_scan(&self, scan_json: Value) -> Result<ScanId> {
        let endpoint = "/scan/register";
        let body = scan_json.to_string();
        let res = self.send_post_req(endpoint, body);
        let res_json = self.parse_response_to_json(res)?;

        if res_json["error"].is_null() {
            let scan_id = res_json["scanId"].to_string().parse::<ScanId>()?;
            Ok(scan_id)
        } else {
            Err(NodeError::BadRequest(res_json["error"].to_string()))
        }
    }

    pub fn deregister_scan(&self, scan_id: ScanId) -> Result<ScanId> {
        let endpoint = "/scan/deregister";
        let body = generate_deregister_scan_json(scan_id);
        let res = self.send_post_req(endpoint, body);
        let res_json = self.parse_response_to_json(res)?;

        if res_json["error"].is_null() {
            let scan_id = res_json["scanId"].to_string().parse::<ScanId>()?;
            Ok(scan_id)
        } else {
            Err(NodeError::BadRequest(res_json["error"].to_string()))
        }
    }

    /// Using the `scan_id` of a registered scan, acquires unspent boxes which have been found by said scan
    pub fn scan_boxes(&self, scan_id: ScanId) -> Result<Vec<ErgoBox>> {
        let endpoint = format!("/scan/unspentBoxes/{scan_id}");
        let res = self.send_get_req(&endpoint);
        let res_json = self.parse_response_to_json(res)?;

        let mut box_list = vec![];
        for i in 0.. {
            let box_json = &res_json[i]["box"];
            if box_json.is_null() {
                break;
            } else {
                let res_ergo_box = from_str(&box_json.to_string());
                if let Ok(ergo_box) = res_ergo_box {
                    box_list.push(ergo_box);
                } else if let Err(e) = res_ergo_box {
                    let mess = format!("Box Json: {box_json}\nError: {e:?}");
                    return Err(NodeError::FailedParsingBox(mess));
                }
            }
        }
        Ok(box_list)
    }

    /// Using the `scan_id` of a registered scan, manually adds a box to said
    /// scan.
    pub fn add_box_to_scan(&self, scan_id: ScanId, box_id: &String) -> Result<String> {
        let ergo_box = serde_json::to_string(&self.box_from_id(box_id)?)
            .map_err(|_| NodeError::FailedParsingBox(box_id.clone()))?;
        let scan_id_int: u64 = scan_id.into();
        let endpoint = "/scan/addBox";

        let body = json! ({
            "scanIds": vec![scan_id_int],
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

fn generate_deregister_scan_json(scan_id: ScanId) -> String {
    let body = json!({
        "scanId": scan_id,
    });
    to_string_pretty(&body).unwrap()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_deregister_scan_json() {
        let scan_id = ScanId::from(100);
        expect_test::expect![[r#"
            {
              "scanId": 100
            }"#]]
        .assert_eq(&generate_deregister_scan_json(scan_id));
    }
}
