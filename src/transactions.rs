use crate::node_interface::{NodeError, NodeInterface, Result};
use crate::JsonString;
use crate::TxId;
use ergo_lib::chain::transaction::unsigned::UnsignedTransaction;
use ergo_lib::chain::transaction::Transaction;
use ergo_lib::ergotree_ir::chain::ergo_box::ErgoBox;
use ergo_lib::wallet::signing::TransactionContext;
use json::JsonValue;
use serde_json::json;

impl NodeInterface {
    /// Submits a Signed Transaction provided as input as JSON
    /// to the Ergo Blockchain mempool.
    pub fn submit_json_transaction(&self, signed_tx_json: &JsonString) -> Result<TxId> {
        let endpoint = "/transactions";
        let res_json = self.use_json_endpoint_and_check_errors(endpoint, signed_tx_json)?;

        // If tx is valid and is posted, return just the tx id
        let tx_id = res_json.dump();
        Ok(tx_id)
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
        self.submit_json_transaction(signed_tx_json)
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
        let prepared_body = json!({ "tx": unsigned_tx, "inputsRaw": boxes_to_spend, "dataInputsRaw": data_input_boxes });

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
        // If tx is valid and is posted, return just the tx id
        let tx_id = res_json.dump();
        Ok(tx_id)
    }

    /// Generates Json of an Unsigned Transaction.
    /// Input must be a json formatted request with rawInputs (and rawDataInputs)
    /// manually selected or will be automatically selected by wallet.
    pub fn generate_json_transaction(&self, tx_request_json: &JsonString) -> Result<JsonValue> {
        let endpoint = "/wallet/transaction/generate";
        let res_json = self.use_json_endpoint_and_check_errors(endpoint, tx_request_json)?;

        Ok(res_json)
    }
}
