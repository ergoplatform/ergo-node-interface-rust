/// Functions related to saving/accessing local data
/// for interacting with an Ergo Node. (Ip/Port/Api Key)
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

/// A ease-of-use function which attempts to acquire a `NodeInterface`
/// from a local file. If the file does not exist, it generates a new
/// config file, tells the user to edit the config file, and then closes
/// the running application
/// This is useful for CLI applications, however should not be used by
/// GUI-based applications.
pub fn acquire_node_interface_from_local_config() -> NodeInterface {
    // `Node-interface.yaml` setup logic
    if !does_local_config_exist() {
        println!("Could not find local `node-interface.yaml` file.\nCreating said file with basic defaults.\nPlease edit the yaml file and update it with your node parameters to ensure the CLI app can proceed.");
        create_new_local_config_file().ok();
        std::process::exit(0);
    }
    // Error checking reading the local node interface yaml
    if let Err(e) = new_interface_from_local_config() {
        println!("Could not parse local `node-interface.yaml` file.\nError: {e:?}");
        std::process::exit(0);
    }
    // Create `NodeInterface`
    new_interface_from_local_config().unwrap()
}

/// Basic function to check if a local config currently exists
pub fn does_local_config_exist() -> bool {
    Path::new("node-interface.yaml").exists()
}

/// Create a new `node-interface.config` with the barebones yaml inside
pub fn create_new_local_config_file() -> Result<()> {
    let file_path = Path::new("node-interface.yaml");
    if !file_path.exists() {
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
    let ip = config["node_ip"].as_str().ok_or_else(|| {
        NodeError::YamlError("`node_ip` is not specified in the provided Yaml".to_string())
    })?;
    let port = config["node_port"].as_str().ok_or_else(|| {
        NodeError::YamlError("`node_port` is not specified in the provided Yaml".to_string())
    })?;
    let api_key = config["node_api_key"].as_str().ok_or_else(|| {
        NodeError::YamlError("`node_api_key` is not specified in the provided Yaml".to_string())
    })?;
    NodeInterface::new(api_key, ip, port)
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
