/// The `NodeInterface` struct is defined which allows for interacting with an
/// Ergo Node via Rust.
use crate::JsonString;
use ergo_lib::chain::ergo_box::ErgoBox;
use ergo_offchain_utilities::{BlockHeight, NanoErg, P2PKAddressString, P2SAddressString, TxId};
use json::JsonValue;
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
}

/// The `NodeInterface` struct which holds the relevant Ergo node data
/// and has methods implemented to interact with the node.
#[derive(Debug, Clone)]
pub struct NodeInterface {
    pub api_key: String,
    pub ip: String,
    pub port: String,
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

    /// Submits a Signed Transaction provided as input as JSON
    /// to the Ergo Blockchain mempool.
    pub fn submit_transaction(&self, signed_tx_json: &JsonString) -> Result<TxId> {
        let endpoint = "/transactions";
        let res_json = self.use_json_endpoint_and_check_errors(endpoint, signed_tx_json)?;

        // If tx is valid and is posted, return just the tx id
        let tx_id = res_json.dump();
        return Ok(tx_id);
    }

    /// Generates Json of an Unsigned Transaction.
    /// Input must be a json formatted request with rawInputs (and rawDataInputs)
    /// manually selected or will be automatically selected by wallet.
    pub fn generate_transaction(&self, tx_request_json: &JsonString) -> Result<JsonValue> {
        let endpoint = "/wallet/transaction/generate";
        let res_json = self.use_json_endpoint_and_check_errors(endpoint, tx_request_json)?;

        Ok(res_json)
    }

    /// Sign an Unsigned Transaction which is formatted in JSON
    pub fn sign_transaction(&self, unsigned_tx_string: &JsonString) -> Result<JsonValue> {
        let endpoint = "/wallet/transaction/sign";
        let unsigned_tx_json = json::parse(&unsigned_tx_string)
            .map_err(|_| NodeError::FailedParsingNodeResponse(unsigned_tx_string.to_string()))?;

        let prepared_body = object! {
            tx: unsigned_tx_json
        };

        println!("Unsigned tx Json: {:?}", prepared_body.dump());

        let res_json = self.use_json_endpoint_and_check_errors(endpoint, &prepared_body.dump())?;

        Ok(res_json)
    }

    /// Sign an Unsigned Transaction which is formatted in JSON
    /// and then submit it to the mempool.
    pub fn sign_and_submit_transaction(&self, unsigned_tx_string: &JsonString) -> Result<TxId> {
        let signed_tx = self.sign_transaction(unsigned_tx_string)?;
        let signed_tx_json = json::stringify(signed_tx);

        self.submit_transaction(&signed_tx_json)
    }

    /// Generates and submits a tx using the node endpoints. Input is
    /// a json formatted request with rawInputs (and rawDataInputs)
    /// manually selected or inputs will be automatically selected by wallet.
    /// Returns the resulting `TxId`.
    pub fn generate_and_submit_transaction(&self, tx_request_json: &JsonString) -> Result<TxId> {
        let endpoint = "/wallet/transaction/send";
        let res_json = self.use_json_endpoint_and_check_errors(endpoint, tx_request_json)?;
        // If tx is valid and is posted, return just the tx id
        let tx_id = res_json.dump();
        return Ok(tx_id);
    }

    /// Get all addresses from the node wallet
    pub fn wallet_addresses(&self) -> Result<Vec<P2PKAddressString>> {
        let endpoint = "/wallet/addresses";
        let res = self.send_get_req(endpoint)?;

        let mut addresses: Vec<String> = vec![];
        for segment in res
            .text()
            .expect("Failed to get addresses from wallet.")
            .split("\"")
        {
            let seg = segment.trim();
            if seg.chars().next().unwrap() == '9' {
                addresses.push(seg.to_string());
            }
        }
        if addresses.len() == 0 {
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
        if let Ok(_) = std::io::stdin().read_line(&mut input) {
            if let Ok(input_n) = input.trim().parse::<usize>() {
                if input_n > address_list.len() || input_n < 1 {
                    println!("Please select an address within the range.");
                    return self.select_wallet_address();
                }
                return Ok(address_list[input_n - 1].clone());
            }
        }
        return self.select_wallet_address();
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
            } else {
                if let Some(ergo_box) = from_str(&box_json.to_string()).ok() {
                    box_list.push(ergo_box);
                }
            }
        }
        Ok(box_list)
    }

    /// Returns unspent boxes from the node wallet ordered from highest to
    /// lowest nanoErgs value.
    pub fn unspent_boxes_sorted(&self) -> Result<Vec<ErgoBox>> {
        let mut boxes = self.unspent_boxes()?;
        boxes.sort_by(|a, b| b.value.as_u64().partial_cmp(&a.value.as_u64()).unwrap());

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
    fn consume_boxes_until_total(
        &self,
        total: NanoErg,
        boxes: &Vec<ErgoBox>,
    ) -> Result<Vec<ErgoBox>> {
        let mut count = 0;
        let filtered_boxes = boxes.into_iter().fold(vec![], |mut acc, b| {
            if count >= total {
                acc
            } else {
                count += b.value.as_u64();
                acc.push(b.clone());
                acc
            }
        });
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
            if b.value.as_u64().clone() > acc {
                b.value.as_u64().clone()
            } else {
                acc
            }
        });

        for b in boxes {
            if b.value.as_u64().clone() == highest_value {
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

    /// Given a P2S Ergo address, extract the hex-encoded serialized ErgoTree (script)
    pub fn p2s_to_tree(&self, address: &P2SAddressString) -> Result<String> {
        let endpoint = "/script/addressToTree/".to_string() + address;
        let res = self.send_get_req(&endpoint);
        let res_json = self.parse_response_to_json(res)?;

        Ok(res_json["tree"].to_string().clone())
    }

    /// Given a P2S Ergo address, convert it to a hex-encoded Sigma byte array constant
    pub fn p2s_to_bytes(&self, address: &P2SAddressString) -> Result<String> {
        let endpoint = "/script/addressToBytes/".to_string() + address;
        let res = self.send_get_req(&endpoint);
        let res_json = self.parse_response_to_json(res)?;

        Ok(res_json["bytes"].to_string().clone())
    }

    /// Given an Ergo P2PK Address, convert it to a raw hex-encoded EC point
    pub fn p2pk_to_raw(&self, address: &P2PKAddressString) -> Result<String> {
        let endpoint = "/utils/addressToRaw/".to_string() + address;
        let res = self.send_get_req(&endpoint);
        let res_json = self.parse_response_to_json(res)?;

        Ok(res_json["raw"].to_string().clone())
    }

    /// Given an Ergo P2PK Address, convert it to a raw hex-encoded EC point
    /// and prepend the type bytes so it is encoded and ready
    /// to be used in a register.
    pub fn p2pk_to_raw_for_register(&self, address: &P2PKAddressString) -> Result<String> {
        let add = self.p2pk_to_raw(address)?;
        Ok("07".to_string() + &add)
    }

    /// Given a raw hex-encoded EC point, convert it to a P2PK address
    pub fn raw_to_p2pk(&self, raw: &String) -> Result<P2PKAddressString> {
        let endpoint = "/utils/rawToAddress/".to_string() + raw;
        let res = self.send_get_req(&endpoint);
        let res_json = self.parse_response_to_json(res)?;

        Ok(res_json["address"].to_string().clone())
    }

    /// Given a raw hex-encoded EC point from a register (thus with type encoded characters in front),
    /// convert it to a P2PK address
    pub fn raw_from_register_to_p2pk(&self, typed_raw: &String) -> Result<P2PKAddressString> {
        Ok(self.raw_to_p2pk(&typed_raw[2..].to_string())?)
    }

    /// Given a `Vec<ErgoBox>` return the given boxes (which must be part of the UTXO-set) as
    /// a vec of serialized strings in Base16 encoding
    pub fn serialize_boxes(&self, b: &Vec<ErgoBox>) -> Result<Vec<String>> {
        Ok(b.iter()
            .map(|b| {
                self.serialized_box_from_id(&b.box_id().into())
                    .unwrap_or("".to_string())
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

        Ok(res_json["bytes"].to_string().clone())
    }

    /// Given a box id return the given box (which must be part of the
    /// UTXO-set) as a serialized string in Base16 encoding
    pub fn box_from_id(&self, box_id: &String) -> Result<ErgoBox> {
        let endpoint = "/utxo/byId/".to_string() + box_id;
        let res = self.send_get_req(&endpoint);
        let res_json = self.parse_response_to_json(res)?;

        if let Some(ergo_box) = from_str(&res_json.to_string()).ok() {
            return Ok(ergo_box);
        } else {
            return Err(NodeError::FailedParsingBox(res_json.pretty(2)));
        }
    }

    /// Get the current nanoErgs balance held in the Ergo Node wallet
    pub fn wallet_nano_ergs_balance(&self) -> Result<NanoErg> {
        let endpoint = "/wallet/balances";
        let res = self.send_get_req(&endpoint);
        let res_json = self.parse_response_to_json(res)?;

        let balance = res_json["balance"].clone();

        if balance.is_null() {
            return Err(NodeError::NodeSyncing);
        } else {
            return balance
                .as_u64()
                .ok_or(NodeError::FailedParsingNodeResponse(res_json.to_string()));
        }
    }

    /// Get the current block height of the blockchain
    pub fn current_block_height(&self) -> Result<BlockHeight> {
        let endpoint = "/info";
        let res = self.send_get_req(&endpoint);
        let res_json = self.parse_response_to_json(res)?;

        let height_json = res_json["fullHeight"].clone();

        if height_json.is_null() {
            return Err(NodeError::NodeSyncing);
        } else {
            return height_json
                .to_string()
                .parse()
                .map_err(|_| NodeError::FailedParsingNodeResponse(res_json.to_string()));
        }
    }
}
