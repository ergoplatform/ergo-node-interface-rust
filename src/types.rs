use std::convert::TryFrom;
use std::str::FromStr;

use derive_more::{Display, From, Into};
use serde::{Deserialize, Deserializer, Serialize};

use crate::node_interface::NodeError;

#[derive(Debug, Copy, Clone, From, Into, Display, Serialize, PartialEq, Eq, Hash)]
pub struct ScanId(u64);

impl TryFrom<ScanId> for i16 {
    type Error = NodeError;

    fn try_from(value: ScanId) -> Result<Self, Self::Error> {
        i16::try_from(value.0).map_err(|_| {
            NodeError::InvalidScanId(format!(
                "{} is outside valid Ergo node i16 range (0..={})",
                value.0,
                i16::MAX
            ))
        })
    }
}

impl FromStr for ScanId {
    type Err = NodeError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let scan_id = s
            .parse::<u64>()
            .map_err(|_| NodeError::InvalidScanId(s.to_string()))?;
        i16::try_from(scan_id).map_err(|_| NodeError::InvalidScanId(s.to_string()))?;
        Ok(ScanId(scan_id))
    }
}

impl<'de> Deserialize<'de> for ScanId {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let raw = u64::deserialize(deserializer)?;
        i16::try_from(raw).map(|_| ScanId(raw)).map_err(|_| {
            serde::de::Error::custom(format!(
                "scan ID {raw} is outside valid Ergo node i16 range (0..={})",
                i16::MAX
            ))
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_scan_id_range_validation() {
        assert_eq!("0".parse::<ScanId>().unwrap(), ScanId::from(0));
        assert_eq!("100".parse::<ScanId>().unwrap(), ScanId::from(100));
        assert_eq!(
            i16::MAX.to_string().parse::<ScanId>().unwrap(),
            ScanId::from(i16::MAX as u64)
        );
        assert_eq!(
            i16::try_from(ScanId::from(i16::MAX as u64)).unwrap(),
            i16::MAX
        );

        assert!(matches!(
            "32768".parse::<ScanId>(),
            Err(NodeError::InvalidScanId(_))
        ));
        assert!(matches!(
            "-1".parse::<ScanId>(),
            Err(NodeError::InvalidScanId(_))
        ));
        assert!(matches!(
            "not-a-number".parse::<ScanId>(),
            Err(NodeError::InvalidScanId(_))
        ));
        assert!(matches!(
            i16::try_from(ScanId::from(i16::MAX as u64 + 1)),
            Err(NodeError::InvalidScanId(_))
        ));
        assert!(matches!(
            i16::try_from(ScanId::from(u64::MAX)),
            Err(NodeError::InvalidScanId(_))
        ));
    }

    #[test]
    fn test_scan_id_serde_range_validation() {
        let valid: ScanId = serde_json::from_str("32767").unwrap();
        assert_eq!(valid, ScanId::from(32767));
        assert_eq!(serde_json::to_string(&valid).unwrap(), "32767");

        assert!(serde_json::from_str::<ScanId>("32768").is_err());
        assert!(serde_json::from_str::<ScanId>("-1").is_err());
    }
}
