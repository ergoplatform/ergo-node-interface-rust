/// A struct `Scan` is defined here which wraps the concept of UTXO-set
/// scanning in a Rust-based struct interface.
use crate::node_interface::NodeInterface;
pub use crate::node_interface::{NodeError, Result};
use ergo_lib::chain::ergo_box::ErgoBox;
use ergo_offchain_utilities::ScanID;
use json;
use json::JsonValue;

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
        let boxes = self.node_interface.get_scan_boxes(&self.id)?;
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
        for scan in scans {
            if &scan.id == "null" {
                return Err(NodeError::FailedRegisteringScan(scan.name));
            }
            id_json[scan.name] = scan.id.into();
        }
        std::fs::write("scanIDs.json", json::stringify_pretty(id_json, 4)).map_err(|_| {
            NodeError::Other("Failed to save scans to local scanIDs.json".to_string())
        })?;
        Ok(true)
    }
}
