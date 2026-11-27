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
    // Create `NodeInterface`
    match new_interface_from_local_config() {
        Ok(node) => node,
        Err(e) => {
            println!("Could not parse local `node-interface.yaml` file.\nError: {e:?}");
            std::process::exit(0);
        }
    }
}

/// Basic function to check if a local config currently exists
pub fn does_local_config_exist() -> bool {
    Path::new("node-interface.yaml").exists()
}

/// Create a new `node-interface.config` with the barebones yaml inside
pub fn create_new_local_config_file() -> Result<()> {
    create_new_local_config_file_at(Path::new("node-interface.yaml"))
}

fn create_new_local_config_file_at(file_path: &Path) -> Result<()> {
    if !file_path.exists() {
        let mut file = File::create(file_path).map_err(|_| {
            NodeError::YamlError("Failed to create `node-interface.yaml` file".to_string())
        })?;
        file.write_all(BAREBONES_CONFIG_YAML.as_bytes())
            .map_err(|_| {
                NodeError::YamlError(
                    "Failed to write to local `node-interface.yaml` file".to_string(),
                )
            })?;
        return Ok(());
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
    new_interface_from_local_config_at(Path::new("node-interface.yaml"))
}

fn new_interface_from_local_config_at(file_path: &Path) -> Result<NodeInterface> {
    let yaml_str = std::fs::read_to_string(file_path).map_err(|_| {
        NodeError::YamlError("Failed to read local `node-interface.yaml` file".to_string())
    })?;
    let docs = YamlLoader::load_from_str(&yaml_str)
        .map_err(|e| NodeError::YamlError(format!("Failed to parse local config YAML: {e}")))?;
    let yaml = docs.into_iter().next().ok_or_else(|| {
        NodeError::YamlError("Local `node-interface.yaml` file is empty".to_string())
    })?;
    new_interface_from_yaml(yaml)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicUsize, Ordering};

    static TEST_COUNTER: AtomicUsize = AtomicUsize::new(0);

    struct TempConfigPath(PathBuf);

    impl TempConfigPath {
        fn new(label: &str) -> Self {
            let id = TEST_COUNTER.fetch_add(1, Ordering::Relaxed);
            let dir = std::env::temp_dir().join(format!(
                "ergo-node-interface-config-test-{}-{}-{label}",
                std::process::id(),
                id
            ));
            std::fs::create_dir_all(&dir).expect("create temp test dir");
            Self(dir.join("node-interface.yaml"))
        }
    }

    impl Drop for TempConfigPath {
        fn drop(&mut self) {
            if let Some(parent) = self.0.parent() {
                let _ = std::fs::remove_dir_all(parent);
            }
        }
    }

    #[test]
    fn test_create_new_local_config_file_returns_ok_and_errors_if_exists() {
        let tmp = TempConfigPath::new("create");
        assert!(create_new_local_config_file_at(&tmp.0).is_ok());
        assert!(tmp.0.exists());

        let node = new_interface_from_local_config_at(&tmp.0)
            .expect("created default config must load cleanly");
        assert_eq!(node.api_key, "hello");
        assert_eq!(node.url.as_str(), "http://0.0.0.0:9053/");

        let second = create_new_local_config_file_at(&tmp.0);
        assert!(matches!(second, Err(NodeError::YamlError(_))));
    }

    #[test]
    fn test_new_interface_from_local_config_handles_empty_and_invalid_yaml() {
        let empty = TempConfigPath::new("empty");
        std::fs::write(&empty.0, "").expect("write empty yaml");
        assert!(matches!(
            new_interface_from_local_config_at(&empty.0),
            Err(NodeError::YamlError(_))
        ));

        let invalid = TempConfigPath::new("invalid");
        std::fs::write(&invalid.0, "node_ip: [unterminated").expect("write invalid yaml");
        assert!(matches!(
            new_interface_from_local_config_at(&invalid.0),
            Err(NodeError::YamlError(_))
        ));
    }
}
