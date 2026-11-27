use crate::node_interface::{NodeError, NodeInterface, Result};
use crate::JsonString;
use json::JsonValue;
use reqwest::blocking::{RequestBuilder, Response};
use reqwest::header::{HeaderValue, CONTENT_TYPE};
use reqwest::StatusCode;

pub(crate) fn format_http_error(status: StatusCode, body: &str) -> String {
    if let Ok(parsed) = json::parse(body) {
        if parsed.is_object() {
            let detail = parsed["detail"]
                .as_str()
                .filter(|s| !s.is_empty())
                .map(str::to_string)
                .or_else(|| {
                    if !parsed["detail"].is_null() {
                        Some(parsed["detail"].to_string())
                    } else {
                        None
                    }
                });
            let reason = parsed["reason"]
                .as_str()
                .filter(|s| !s.is_empty())
                .map(str::to_string);
            let error = parsed["error"]
                .as_str()
                .filter(|s| !s.is_empty())
                .map(str::to_string);

            return match (reason, detail) {
                (Some(r), Some(d)) => format!("HTTP {status} ({r}): {d}"),
                (None, Some(d)) => format!("HTTP {status}: {d}"),
                (Some(r), None) => format!("HTTP {status} ({r})"),
                (None, None) => match error {
                    Some(e) => format!("HTTP {status}: {e}"),
                    None => format!("HTTP {status}: {body}"),
                },
            };
        }
    }
    if body.trim().is_empty() {
        format!("HTTP {status}")
    } else {
        format!("HTTP {status}: {body}")
    }
}

impl NodeInterface {
    /// Builds a `HeaderValue` to use for requests with the api key specified
    pub fn get_node_api_header(&self) -> HeaderValue {
        match HeaderValue::from_str(&self.api_key) {
            Ok(k) => k,
            _ => HeaderValue::from_static("None"),
        }
    }

    /// Sets required headers for a request
    pub fn set_req_headers(&self, rb: RequestBuilder) -> RequestBuilder {
        rb.header("accept", "application/json")
            .header("api_key", self.get_node_api_header())
            .header(CONTENT_TYPE, "application/json")
    }

    /// Sends a GET request to the Ergo node
    pub fn send_get_req(&self, endpoint: &str) -> Result<Response> {
        let url = self
            .url
            .join(endpoint)
            .map_err(|e| NodeError::InvalidUrl(e.to_string()))?;
        let client = reqwest::blocking::Client::new().get(url);
        self.set_req_headers(client)
            .send()
            .map_err(|_| NodeError::NodeUnreachable)
    }

    /// Sends a POST request to the Ergo node
    pub fn send_post_req(&self, endpoint: &str, body: String) -> Result<Response> {
        let url = self
            .url
            .join(endpoint)
            .map_err(|e| NodeError::InvalidUrl(e.to_string()))?;
        let client = reqwest::blocking::Client::new().post(url);
        self.set_req_headers(client)
            .body(body)
            .send()
            .map_err(|_| NodeError::NodeUnreachable)
    }

    /// Parses response from node into JSON
    pub fn parse_response_to_json(&self, resp: Result<Response>) -> Result<JsonValue> {
        let resp = resp?;
        let status = resp.status();
        let text = resp.text().map_err(|_| {
            NodeError::FailedParsingNodeResponse(
                "Node Response Not Parseable into Text.".to_string(),
            )
        })?;
        if !status.is_success() {
            return Err(NodeError::BadRequest(format_http_error(status, &text)));
        }
        let json = json::parse(&text).map_err(|_| NodeError::FailedParsingNodeResponse(text))?;
        Ok(json)
    }

    /// General function for submitting a Json String body to an endpoint
    /// which also returns a `JsonValue` response.
    pub fn use_json_endpoint_and_check_errors(
        &self,
        endpoint: &str,
        json_body: &JsonString,
    ) -> Result<JsonValue> {
        let res = self.send_post_req(endpoint, json_body.to_string());
        let res_json = self.parse_response_to_json(res)?;

        if res_json.is_object() {
            if !res_json["detail"].is_null() {
                return Err(NodeError::BadRequest(res_json["detail"].to_string()));
            }
            if res_json["error"].is_number()
                || res_json["error"]
                    .as_str()
                    .map(|s| !s.is_empty())
                    .unwrap_or(false)
            {
                return Err(NodeError::BadRequest(res_json.to_string()));
            }
        }

        Ok(res_json)
    }
}

#[cfg(test)]
pub(crate) mod test_support {
    use crate::node_interface::NodeInterface;
    use ergo_lib::ergotree_ir::chain::address::AddressEncoder;
    use ergo_lib::ergotree_ir::chain::ergo_box::box_value::BoxValue;
    use ergo_lib::ergotree_ir::chain::ergo_box::{ErgoBox, NonMandatoryRegisters};
    use ergo_lib::ergotree_ir::chain::tx_id::TxId;
    use std::convert::TryFrom;
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::thread::{self, JoinHandle};
    use std::time::Duration;

    pub(crate) const MAINNET_P2PK: &str = "9fRAWhdxEsTcdb8PhGNrZfwqa65zfkuYHAMmkQLcic1gdLSV5vA";
    pub(crate) const TESTNET_P2PK: &str = "3WvsT2Gm4EpsM9Pg18PdY6XyhNNMqXDsvJTbbf6ihLvAmSb7u5RN";
    pub(crate) const MAINNET_P2S: &str = "4MQyML64GnzMxZgm";
    pub(crate) const TESTNET_P2S: &str = "Ms7smJwLGbUAjuWQ";
    pub(crate) const MAINNET_P2SH: &str = "8UApt8czfFVuTgQmMwtsRBZ4nfWquNiSwCWUjMg";
    pub(crate) const TESTNET_P2SH: &str = "rbcrmKEYduUvADj9Ts3dSVSG27h54pgrq5fPuwB";

    #[derive(Debug, Clone)]
    pub(crate) struct MockRequest {
        pub(crate) method: String,
        pub(crate) target: String,
        pub(crate) body: String,
    }

    pub(crate) struct MockResponse {
        pub(crate) status: u16,
        pub(crate) reason: &'static str,
        pub(crate) body: String,
    }

    impl MockResponse {
        pub(crate) fn json(status: u16, reason: &'static str, body: impl Into<String>) -> Self {
            Self {
                status,
                reason,
                body: body.into(),
            }
        }

        pub(crate) fn ok(body: impl Into<String>) -> Self {
            Self::json(200, "OK", body)
        }
    }

    pub(crate) struct MockServer {
        pub(crate) node: NodeInterface,
        handle: Option<JoinHandle<Vec<MockRequest>>>,
    }

    impl MockServer {
        pub(crate) fn spawn<F>(expected_requests: usize, handler: F) -> Self
        where
            F: Fn(&MockRequest) -> MockResponse + Send + 'static,
        {
            let listener = TcpListener::bind("127.0.0.1:0").expect("bind loopback listener");
            let addr = listener.local_addr().expect("listener local addr");
            let node = NodeInterface::from_url_str("test-api-key", &format!("http://{addr}/"))
                .expect("valid loopback URL");

            let handle = thread::spawn(move || {
                let mut recorded = Vec::with_capacity(expected_requests);
                for _ in 0..expected_requests {
                    let (mut stream, _) = listener.accept().expect("accept client connection");
                    stream
                        .set_read_timeout(Some(Duration::from_secs(5)))
                        .expect("set read timeout");

                    let req = read_http_request(&mut stream);
                    let resp = handler(&req);
                    recorded.push(req);

                    let response_bytes = format!(
                        "HTTP/1.1 {} {}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                        resp.status,
                        resp.reason,
                        resp.body.len(),
                        resp.body
                    );
                    stream
                        .write_all(response_bytes.as_bytes())
                        .expect("write HTTP response");
                    stream.flush().expect("flush HTTP response");
                }
                recorded
            });

            Self {
                node,
                handle: Some(handle),
            }
        }

        pub(crate) fn finish(mut self) -> Vec<MockRequest> {
            self.handle
                .take()
                .expect("server handle present")
                .join()
                .expect("mock server thread completed cleanly")
        }
    }

    impl Drop for MockServer {
        fn drop(&mut self) {
            if let Some(handle) = self.handle.take() {
                if !thread::panicking() {
                    handle.join().expect("mock server thread completed cleanly");
                }
            }
        }
    }

    fn read_http_request(stream: &mut impl Read) -> MockRequest {
        let mut buf = Vec::new();
        let mut chunk = [0u8; 1024];
        let header_end = loop {
            let n = stream.read(&mut chunk).expect("read HTTP request");
            assert!(n > 0, "unexpected EOF while reading HTTP headers");
            buf.extend_from_slice(&chunk[..n]);
            if let Some(pos) = buf.windows(4).position(|w| w == b"\r\n\r\n") {
                break pos + 4;
            }
        };

        let header_text = String::from_utf8_lossy(&buf[..header_end]).to_string();
        let mut lines = header_text.split("\r\n");
        let request_line = lines.next().expect("HTTP request line");
        let mut parts = request_line.split_whitespace();
        let method = parts.next().expect("HTTP method").to_string();
        let target = parts.next().expect("HTTP request target").to_string();

        let content_length = lines
            .filter_map(|line| {
                let mut kv = line.splitn(2, ':');
                let k = kv.next()?.trim();
                let v = kv.next()?.trim();
                if k.eq_ignore_ascii_case("content-length") {
                    v.parse::<usize>().ok()
                } else {
                    None
                }
            })
            .next()
            .unwrap_or(0);

        while buf.len() < header_end + content_length {
            let n = stream.read(&mut chunk).expect("read HTTP request body");
            assert!(n > 0, "unexpected EOF while reading HTTP body");
            buf.extend_from_slice(&chunk[..n]);
        }

        let body =
            String::from_utf8_lossy(&buf[header_end..header_end + content_length]).to_string();
        MockRequest {
            method,
            target,
            body,
        }
    }

    pub(crate) fn make_test_box(value: u64, index: u16) -> ErgoBox {
        let tree = AddressEncoder::unchecked_parse_network_address_from_str(MAINNET_P2PK)
            .expect("valid mainnet p2pk address")
            .address()
            .script()
            .expect("address script");
        ErgoBox::new(
            BoxValue::try_from(value).expect("valid box value"),
            tree,
            None,
            NonMandatoryRegisters::empty(),
            100_000,
            TxId::zero(),
            index,
        )
        .expect("construct ErgoBox")
    }
}

#[cfg(test)]
mod tests {
    use super::test_support::{MockResponse, MockServer};
    use crate::node_interface::NodeError;

    #[test]
    fn test_non_2xx_with_null_detail_returns_bad_request() {
        for (status, reason, body) in [
            (
                400,
                "Bad Request",
                r#"{"error":400,"reason":"bad.request","detail":null}"#,
            ),
            (
                403,
                "Forbidden",
                r#"{"error":403,"reason":"invalid.api-key","detail":null}"#,
            ),
            (
                404,
                "Not Found",
                r#"{"error":404,"reason":"not-found","detail":null}"#,
            ),
            (
                400,
                "Bad Request",
                r#"{"error":400,"reason":"bad.request"}"#,
            ),
            (500, "Internal Server Error", "plain error text"),
        ] {
            let body_owned = body.to_string();
            let server =
                MockServer::spawn(1, move |_| MockResponse::json(status, reason, &body_owned));
            let err = server
                .node
                .use_json_endpoint_and_check_errors("/transactions", &"{}".to_string())
                .expect_err("non-2xx response must error");
            assert!(
                matches!(err, NodeError::BadRequest(ref msg) if msg.contains(&status.to_string())),
                "unexpected error for status {}: {:?}",
                status,
                err
            );
            server.finish();
        }
    }

    #[test]
    fn test_parse_response_to_json_rejects_non_2xx() {
        let server = MockServer::spawn(1, |_| {
            MockResponse::json(
                403,
                "Forbidden",
                r#"{"error":403,"reason":"invalid.api-key","detail":null}"#,
            )
        });
        let res = server.node.send_get_req("/info");
        let err = server
            .node
            .parse_response_to_json(res)
            .expect_err("403 must return Err");
        assert!(matches!(err, NodeError::BadRequest(ref msg) if msg.contains("403")));
        server.finish();
    }
}
