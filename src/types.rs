use std::str::FromStr;

use derive_more::{Display, From, Into};
use serde::{Deserialize, Serialize};

use crate::node_interface::NodeError;

#[derive(Debug, Copy, Clone, From, Into, Display, Serialize, Deserialize)]
pub struct ScanId(u64);

impl FromStr for ScanId {
    type Err = NodeError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let scan_id = s
            .parse::<u64>()
            .map_err(|_| NodeError::InvalidScanId(s.to_string()))?;
        Ok(ScanId(scan_id))
    }
}
