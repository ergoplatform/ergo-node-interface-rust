/// A struct `Scan` is defined here which wraps the concept of UTXO-set
/// scanning in a Rust-based struct interface.
use crate::node_interface::NodeInterface;
pub use crate::node_interface::{NodeError, Result};
use ergo_lib::chain::ergo_box::ErgoBox;
use ergo_offchain_utilities::ScanID;
use ergo_offchain_utilities::{P2PKAddressString, P2SAddressString};
use json;
use json::JsonValue;
use serde_json::from_str;

/// A `Scan` is a name + scan_id for a given scan with extra methods for acquiring boxes.
#[derive(Debug, Clone)]
pub struct Scan {
    pub name: String,
    pub id: ScanID,
    pub node_interface: NodeInterface,
}

impl Scan {
    /// Manually create a new `Scan` struct. It is assumed that
    /// a scan with the given `id` has already been registered
    /// with the Ergo Node and the developer is simply creating
    /// a struct for the given scan.
    pub fn new(name: &str, scan_id: &str, node_interface: &NodeInterface) -> Scan {
        Scan {
            name: name.to_string(),
            id: scan_id.to_string(),
            node_interface: node_interface.clone(),
        }
    }

    /// Register a new scan in the Ergo Node and builds/returns
    /// a `Scan` struct in a `Result`.
    pub fn register(
        name: &String,
        tracking_rule: JsonValue,
        node_interface: &NodeInterface,
    ) -> Result<Scan> {
        let scan_json = object! {
        scanName: name.clone(),
        trackingRule: tracking_rule.clone(),
        };

        let scan_id = node_interface.register_scan(&scan_json)?;
        return Ok(Scan::new(name, &scan_id, node_interface));
    }

    /// Returns all `ErgoBox`es found by the scan
    pub fn get_boxes(&self) -> Result<Vec<ErgoBox>> {
        let boxes = self.node_interface.scan_boxes(&self.id)?;
        Ok(boxes)
    }

    /// Returns the first `ErgoBox` found by the scan
    pub fn get_box(&self) -> Result<ErgoBox> {
        self.get_boxes()?
            .into_iter()
            .nth(0)
            .ok_or(NodeError::NoBoxesFound)
    }

    /// Returns all `ErgoBox`es found by the scan
    /// serialized and ready to be used as rawInputs
    pub fn get_serialized_boxes(&self) -> Result<Vec<String>> {
        let boxes = self.node_interface.serialize_boxes(&self.get_boxes()?)?;
        Ok(boxes)
    }

    /// Returns the first `ErgoBox` found by the registered scan
    /// serialized and ready to be used as a rawInput
    pub fn get_serialized_box(&self) -> Result<String> {
        let ser_box = self.node_interface.serialize_box(&self.get_box()?)?;
        Ok(ser_box)
    }

    /// Saves UTXO-set scans (specifically id) to local scanIDs.json
    pub fn save_scan_ids_locally(scans: Vec<Scan>) -> Result<bool> {
        let mut id_json = object! {};
        let mut json_list: Vec<JsonValue> = vec![];
        for scan in scans {
            if &scan.id == "null" {
                return Err(NodeError::FailedRegisteringScan(scan.name));
            }
            let sub_json = object! {name: scan.name, id: scan.id};
            json_list.push(sub_json);
        }
        id_json["scans"] = json_list.into();
        std::fs::write("scanIDs.json", json::stringify_pretty(id_json, 4)).map_err(|_| {
            NodeError::Other("Failed to save scans to local scanIDs.json".to_string())
        })?;
        Ok(true)
    }

    /// Read UTXO-set scan ids from local scanIDs.json
    pub fn read_local_scan_ids(node: &NodeInterface) -> Result<Vec<Scan>> {
        let file_string = &std::fs::read_to_string("scanIDs.json")
            .map_err(|_| NodeError::Other("Unable to read scanIDs.json".to_string()))?;
        let scan_json = json::parse(file_string)
            .map_err(|_| NodeError::Other("Failed to parse scanIDs.json".to_string()))?;

        let scans: &Vec<Scan> = &scan_json["scans"]
            .members()
            .map(|scan| Scan::new(&scan["name"].to_string(), &scan["id"].to_string(), node))
            .collect();

        Ok(scans.clone())
    }

    /// Serialize a "P2PKAddressString" to be used within a scan tracking rule
    pub fn serialize_p2pk_for_tracking(
        node: &NodeInterface,
        address: &P2PKAddressString,
    ) -> Result<String> {
        let raw = node.p2pk_to_raw(&address)?;
        Ok("0e240008cd".to_string() + &raw)
    }
}

/// Scanning-related endpoints
impl NodeInterface {
    /// Registers a scan with the node and either returns the `scan_id`
    /// or an error
    pub fn register_scan(&self, scan_json: &JsonValue) -> Result<ScanID> {
        let endpoint = "/scan/register";
        let body = scan_json.clone().to_string();
        let res = self.send_post_req(endpoint, body);
        let res_json = self.parse_response_to_json(res)?;

        if res_json["error"].is_null() {
            return Ok(res_json["scanId"].to_string().clone());
        } else {
            return Err(NodeError::BadRequest(res_json["error"].to_string()));
        }
    }

    /// Using the `scan_id` of a registered scan, acquires unspent boxes which have been found by said scan
    pub fn scan_boxes(&self, scan_id: &ScanID) -> Result<Vec<ErgoBox>> {
        let endpoint = "/scan/unspentBoxes/".to_string() + scan_id;
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
                    let mess = format!("Box Json: {}\nError: {:?}", box_json.to_string(), e);
                    return Err(NodeError::FailedParsingBox(mess));
                }
            }
        }
        Ok(box_list)
    }

    /// Using the `scan_id` of a registered scan, manually adds a box to said
    /// scan.
    pub fn add_box_to_scan(&self, scan_id: &ScanID, box_id: &String) -> Result<String> {
        let ergo_box = serde_json::to_string(&self.box_from_id(box_id)?)
            .map_err(|_| NodeError::FailedParsingBox(box_id.clone()))?;

        let scan_id_int: u64 = scan_id
            .parse()
            .map_err(|_| NodeError::Other("Scan ID was not a valid integer number.".to_string()))?;

        let endpoint = "/scan/addBox";
        let body = object! {
            "scanIds": vec![scan_id_int],
            "box": ergo_box,
        };

        let res = self.send_post_req(endpoint, body.to_string());
        let res_json = self.parse_response_to_json(res)?;

        if res_json["error"].is_null() {
            return Ok(res_json.to_string());
        } else {
            return Err(NodeError::BadRequest(res_json["error"].to_string()));
        }
    }
}
