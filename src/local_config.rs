/// This file holds functions related to saving/accessing local data
/// related to interacting with an ergo node. (Ip/Port/Api Key)
use crate::node_interface::{NodeError, NodeInterface, Result};
use std::fs::File;
use std::io::prelude::*;
use std::path::Path;
use yaml_rust::{Yaml, YamlLoader};

static BAREBONES_CONFIG_YAML: &str = r#"
# IP Address of the node (default is local, edit if yours is different)
node_ip: "0.0.0.0"
# Port that the node is on (default is 9053, edit if yours is different)
node_port: "9053"
# API key for the node (edit if yours is different)
node_api_key: "hello"
"#;

/// Basic function to check if a local config currently exists
pub fn does_local_config_exist() -> bool {
    Path::new("node-interface.yaml").exists()
}

/// Create a new `node-interface.config` with the barebones yaml inside
pub fn create_new_local_config_file() -> Result<()> {
    let file_path = Path::new("node-interface.yaml");
    if file_path.exists() == false {
        let mut file = File::create(file_path).map_err(|_| {
            NodeError::YamlError("Failed to create `node-interface.yaml` file".to_string())
        })?;
        file.write_all(&BAREBONES_CONFIG_YAML.to_string().into_bytes())
            .map_err(|_| {
                NodeError::YamlError(
                    "Failed to write to local `node-interface.yaml` file".to_string(),
                )
            })?;
    }
    Err(NodeError::YamlError(
        "Local `node-interface.yaml` already exists.".to_string(),
    ))
}

/// Uses the config yaml provided to create a new `NodeInterface`
pub fn new_interface_from_yaml(config: Yaml) -> Result<NodeInterface> {
    let ip = config["node_ip"].as_str().ok_or(NodeError::YamlError(
        "`node_ip` is not specified in the provided Yaml".to_string(),
    ))?;
    let port = config["node_port"].as_str().ok_or(NodeError::YamlError(
        "`node_port` is not specified in the provided Yaml".to_string(),
    ))?;
    let api_key = config["node_api_key"].as_str().ok_or(NodeError::YamlError(
        "`node_api_key` is not specified in the provided Yaml".to_string(),
    ))?;
    Ok(NodeInterface::new(api_key, ip, port))
}

/// Opens a local `node-interface.yaml` file and uses the
/// data inside to create a `NodeInterface`
pub fn new_interface_from_local_config() -> Result<NodeInterface> {
    let yaml_str = std::fs::read_to_string("node-interface.yaml").map_err(|_| {
        NodeError::YamlError("Failed to read local `node-interface.yaml` file".to_string())
    })?;
    let yaml = YamlLoader::load_from_str(&yaml_str).unwrap()[0].clone();
    new_interface_from_yaml(yaml)
}
