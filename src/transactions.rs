use std::convert::TryFrom;

use crate::node_interface::{NodeError, NodeInterface, Result};
use crate::JsonString;
use ergo_lib::chain::transaction::unsigned::UnsignedTransaction;
use ergo_lib::chain::transaction::{Transaction, TxId};
use ergo_lib::ergo_chain_types::Digest32;
use ergo_lib::ergotree_ir::chain::ergo_box::ErgoBox;
use ergo_lib::ergotree_ir::serialization::{SigmaSerializable, SigmaSerializationError};
use ergo_lib::wallet::signing::TransactionContext;
use json::JsonValue;
use serde_json::json;

impl NodeInterface {
    /// Submits a Signed Transaction provided as input as JSON
    /// to the Ergo Blockchain mempool.
    pub fn submit_json_transaction(&self, signed_tx_json: &JsonString) -> Result<TxId> {
        let endpoint = "/transactions";
        let res_json = self.use_json_endpoint_and_check_errors(endpoint, signed_tx_json)?;
        parse_tx_id(res_json)
    }

    /// Sign an Unsigned Transaction which is formatted in JSON
    pub fn sign_json_transaction(&self, unsigned_tx_string: &JsonString) -> Result<JsonValue> {
        let endpoint = "/wallet/transaction/sign";
        let unsigned_tx_json = json::parse(unsigned_tx_string)
            .map_err(|_| NodeError::FailedParsingNodeResponse(unsigned_tx_string.to_string()))?;

        let prepared_body = object! {
            tx: unsigned_tx_json
        };

        let res_json = self.use_json_endpoint_and_check_errors(endpoint, &prepared_body.dump())?;

        Ok(res_json)
    }

    /// Sign an Unsigned Transaction which is formatted in JSON
    /// and then submit it to the mempool.
    pub fn sign_and_submit_json_transaction(
        &self,
        unsigned_tx_string: &JsonString,
    ) -> Result<TxId> {
        let signed_tx = self.sign_json_transaction(unsigned_tx_string)?;
        let signed_tx_json = json::stringify(signed_tx);

        self.submit_json_transaction(&signed_tx_json)
    }

    /// Submits a Signed `Transaction` provided as input
    /// to the Ergo Blockchain mempool.
    pub fn submit_transaction(&self, signed_tx: &Transaction) -> Result<TxId> {
        let signed_tx_json = &serde_json::to_string(&signed_tx)
            .map_err(|_| NodeError::Other("Failed Converting `Transaction` to json".to_string()))?;
        let tx_id = self.submit_json_transaction(signed_tx_json)?;
        if tx_id != signed_tx.id() {
            return Err(NodeError::FailedParsingNodeResponse(format!(
                "Submitted transaction id mismatch: expected {:?}, got {:?}",
                signed_tx.id(),
                tx_id
            )));
        }
        Ok(tx_id)
    }

    /// Sign an `UnsignedTransaction`
    /// unsigned_tx - The unsigned transaction to sign.
    /// boxes_to_spend - optional list of input boxes. If not provided, the node will search for the boxes in UTXO
    /// data_input_boxes - optional list of data boxes. If not provided, the node will search for the data boxes in UTXO
    pub fn sign_transaction(
        &self,
        unsigned_tx: &UnsignedTransaction,
        boxes_to_spend: Option<Vec<ErgoBox>>,
        data_input_boxes: Option<Vec<ErgoBox>>,
    ) -> Result<Transaction> {
        if let Some(ref boxes_to_spend) = boxes_to_spend {
            // check input boxes against tx's inputs (for every input should be a box)
            if let Err(e) = TransactionContext::new(
                unsigned_tx.clone(),
                boxes_to_spend.clone(),
                data_input_boxes.clone().unwrap_or_default(),
            ) {
                return Err(NodeError::Other(e.to_string()));
            };
        }

        let endpoint = "/wallet/transaction/sign";

        fn encode_boxes(
            maybe_boxes: Option<Vec<ErgoBox>>,
        ) -> std::result::Result<Option<Vec<String>>, NodeError> {
            match maybe_boxes.map(|boxes| {
                boxes
                    .iter()
                    .map(|b| {
                        b.sigma_serialize_bytes()
                            .map(|bytes| base16::encode_lower(&bytes))
                    })
                    .collect::<std::result::Result<Vec<String>, SigmaSerializationError>>()
            }) {
                Some(Ok(base16_boxes)) => Ok(Some(base16_boxes)),
                Some(Err(e)) => Err(NodeError::Other(e.to_string())),
                None => Ok(None),
            }
        }

        let input_boxes_base16 = encode_boxes(boxes_to_spend)?;
        let data_input_boxes_base16 = encode_boxes(data_input_boxes)?;

        let prepared_body = json!({
            "tx": unsigned_tx,
            "inputsRaw": input_boxes_base16,
            "dataInputsRaw": data_input_boxes_base16,
        });

        let json_signed_tx =
            self.use_json_endpoint_and_check_errors(endpoint, &prepared_body.to_string())?;

        serde_json::from_str(&json_signed_tx.dump())
            .map_err(|_| NodeError::Other("Failed Converting `Transaction` to json".to_string()))
    }

    /// Sign an `UnsignedTransaction` and then submit it to the mempool.
    pub fn sign_and_submit_transaction(&self, unsigned_tx: &UnsignedTransaction) -> Result<TxId> {
        let signed_tx = self.sign_transaction(unsigned_tx, None, None)?;
        self.submit_transaction(&signed_tx)
    }

    /// Generates and submits a tx using the node endpoints. Input is
    /// a json formatted request with rawInputs (and rawDataInputs)
    /// manually selected or inputs will be automatically selected by wallet.
    /// Returns the resulting `TxId`.
    pub fn generate_and_submit_transaction(&self, tx_request_json: &JsonString) -> Result<TxId> {
        let endpoint = "/wallet/transaction/send";
        let res_json = self.use_json_endpoint_and_check_errors(endpoint, tx_request_json)?;
        parse_tx_id(res_json)
    }

    /// Generates Json of an Unsigned Transaction.
    /// Input must be a json formatted request with rawInputs (and rawDataInputs)
    /// manually selected or will be automatically selected by wallet.
    pub fn generate_json_transaction(&self, tx_request_json: &JsonString) -> Result<JsonValue> {
        let endpoint = "/wallet/transaction/generate";
        let res_json = self.use_json_endpoint_and_check_errors(endpoint, tx_request_json)?;

        Ok(res_json)
    }

    /// Gets the recommended fee for a transaction.
    /// bytes - size of the transaction in bytes (must be within `1..=i32::MAX`)
    /// wait_time - minutes to wait for the transaction to be included in the blockchain (must be within `1..=i32::MAX`)
    pub fn get_recommended_fee(&self, bytes: u64, wait_time: u64) -> Result<u64> {
        let tx_size = i32::try_from(bytes)
            .ok()
            .filter(|&v| v >= 1)
            .ok_or_else(|| {
                NodeError::BadRequest(format!(
                    "Invalid transaction size `bytes`: {bytes} (expected 1..={})",
                    i32::MAX
                ))
            })?;
        let wait_minutes = i32::try_from(wait_time)
            .ok()
            .filter(|&v| v >= 1)
            .ok_or_else(|| {
                NodeError::BadRequest(format!(
                    "Invalid `wait_time`: {wait_time} (expected 1..={})",
                    i32::MAX
                ))
            })?;
        let endpoint = format!(
            "/transactions/getFee?txSize={}&waitTime={}",
            tx_size, wait_minutes
        );
        let res = self.send_get_req(&endpoint);
        let res_json = self.parse_response_to_json(res)?;
        res_json
            .as_u64()
            .ok_or_else(|| NodeError::FailedParsingNodeResponse(res_json.to_string()))
    }
}

fn parse_tx_id(mut res_json: JsonValue) -> Result<TxId> {
    let raw = res_json.to_string();
    let tx_id_str = res_json
        .take_string()
        .ok_or_else(|| NodeError::FailedParsingNodeResponse(raw.clone()))?;
    let digest = Digest32::try_from(tx_id_str)
        .map_err(|e| NodeError::FailedParsingNodeResponse(format!("Invalid tx id ({raw}): {e}")))?;
    Ok(TxId(digest))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::requests::test_support::{make_test_box, MockResponse, MockServer};
    use ergo_lib::chain::transaction::input::prover_result::ProverResult;
    use ergo_lib::chain::transaction::Input;
    use ergo_lib::ergotree_interpreter::sigma_protocol::prover::{ContextExtension, ProofBytes};
    use ergo_lib::ergotree_ir::chain::ergo_box::ErgoBoxCandidate;

    fn make_test_transaction() -> Transaction {
        let b = make_test_box(1_000_000, 0);
        let input = Input {
            box_id: b.box_id(),
            spending_proof: ProverResult {
                proof: ProofBytes::Empty,
                extension: ContextExtension::empty(),
            },
        };
        let output = ErgoBoxCandidate {
            value: b.value,
            ergo_tree: b.ergo_tree,
            tokens: None,
            additional_registers: b.additional_registers,
            creation_height: b.creation_height,
        };
        Transaction::new_from_vec(vec![input], vec![], vec![output]).expect("construct test tx")
    }

    #[test]
    fn test_get_recommended_fee_uses_tx_size_query_param() {
        let server = MockServer::spawn(1, |req| {
            assert_eq!(req.method, "GET");
            assert_eq!(req.target, "/transactions/getFee?txSize=1024&waitTime=2");
            MockResponse::ok("1100000")
        });

        let fee = server
            .node
            .get_recommended_fee(1024, 2)
            .expect("recommended fee should succeed");
        assert_eq!(fee, 1_100_000);
        server.finish();
    }

    #[test]
    fn test_get_recommended_fee_validates_bounds() {
        let node = NodeInterface::from_url_str("k", "http://127.0.0.1:9/").unwrap();
        assert!(matches!(
            node.get_recommended_fee(0, 1),
            Err(NodeError::BadRequest(_))
        ));
        assert!(matches!(
            node.get_recommended_fee(i32::MAX as u64 + 1, 1),
            Err(NodeError::BadRequest(_))
        ));
        assert!(matches!(
            node.get_recommended_fee(100, 0),
            Err(NodeError::BadRequest(_))
        ));
        assert!(matches!(
            node.get_recommended_fee(100, i32::MAX as u64 + 1),
            Err(NodeError::BadRequest(_))
        ));
    }

    #[test]
    fn test_get_recommended_fee_handles_http_and_parse_errors_without_panic() {
        let server = MockServer::spawn(2, |req| {
            if req.target.contains("txSize=200") {
                MockResponse::json(
                    400,
                    "Bad Request",
                    r#"{"error":400,"reason":"bad.request","detail":null}"#,
                )
            } else {
                MockResponse::ok(r#"{"unexpected":"object"}"#)
            }
        });

        assert!(matches!(
            server.node.get_recommended_fee(200, 1),
            Err(NodeError::BadRequest(_))
        ));
        assert!(matches!(
            server.node.get_recommended_fee(300, 1),
            Err(NodeError::FailedParsingNodeResponse(_))
        ));
        server.finish();
    }

    #[test]
    fn test_submit_json_transaction_handles_invalid_response_without_panic() {
        let server = MockServer::spawn(3, |req| {
            assert_eq!(req.target, "/transactions");
            if req.body.contains("case1") {
                MockResponse::json(
                    403,
                    "Forbidden",
                    r#"{"error":403,"reason":"invalid.api-key","detail":null}"#,
                )
            } else if req.body.contains("case2") {
                MockResponse::ok(r#"{"not":"a-string"}"#)
            } else {
                MockResponse::ok(r#""not-a-hex-tx-id""#)
            }
        });

        assert!(matches!(
            server
                .node
                .submit_json_transaction(&r#"{"case1":true}"#.to_string()),
            Err(NodeError::BadRequest(_))
        ));
        assert!(matches!(
            server
                .node
                .submit_json_transaction(&r#"{"case2":true}"#.to_string()),
            Err(NodeError::FailedParsingNodeResponse(_))
        ));
        assert!(matches!(
            server
                .node
                .submit_json_transaction(&r#"{"case3":true}"#.to_string()),
            Err(NodeError::FailedParsingNodeResponse(_))
        ));
        server.finish();
    }

    #[test]
    fn test_submit_transaction_returns_error_on_tx_id_mismatch() {
        let tx = make_test_transaction();
        let different_id = "00".repeat(32);
        assert_ne!(
            TxId(Digest32::try_from(different_id.clone()).unwrap()),
            tx.id()
        );

        let response_body = format!(r#""{different_id}""#);
        let server = MockServer::spawn(1, move |_| MockResponse::ok(&response_body));

        let err = server
            .node
            .submit_transaction(&tx)
            .expect_err("mismatched tx id must return Err rather than panic");
        assert!(matches!(err, NodeError::FailedParsingNodeResponse(_)));
        server.finish();
    }

    #[test]
    fn test_generate_and_submit_transaction_handles_http_errors_without_panic() {
        let server = MockServer::spawn(1, |req| {
            assert_eq!(req.target, "/wallet/transaction/send");
            MockResponse::json(
                404,
                "Not Found",
                r#"{"error":404,"reason":"not-found","detail":null}"#,
            )
        });

        assert!(matches!(
            server
                .node
                .generate_and_submit_transaction(&"{}".to_string()),
            Err(NodeError::BadRequest(_))
        ));
        server.finish();
    }
}
