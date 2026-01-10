//! Configuration management for LSP server specification.
//!
//! This module handles parsing and validating configuration from both JSON files
//! and command-line arguments.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result, anyhow};
use serde::Deserialize;

#[derive(Debug, Deserialize, Clone)]
pub struct Config {
    pub server: ServerConfig,
}

#[derive(Debug, Deserialize, Clone)]
pub struct ServerConfig {
    pub extensions: Vec<String>,
    pub command: Vec<String>,
    #[serde(rename = "rootDir")]
    pub root_dir: PathBuf,
}

impl Config {
    pub fn from_file(path: &Path) -> Result<Self> {
        let content = std::fs::read_to_string(path)
            .with_context(|| format!("failed to read config file: {}", path.display()))?;
        Self::from_json_str(&content)
    }

    pub fn from_json_str(json: &str) -> Result<Self> {
        let config: Config = serde_json::from_str(json).context("failed to parse config JSON")?;
        config.validate()?;
        Ok(config)
    }

    /// Builds a configuration from command-line server specification.
    pub fn from_server_spec(spec: crate::args::ServerSpec) -> Result<Self> {
        let server = ServerConfig {
            extensions: spec.extensions,
            command: spec.command,
            root_dir: PathBuf::from("."),
        };

        let config = Config { server };
        config.validate()?;
        Ok(config)
    }

    fn validate(&self) -> Result<()> {
        if self.server.extensions.is_empty() {
            return Err(anyhow!("server has no extensions"));
        }
        if self.server.command.is_empty() {
            return Err(anyhow!("server has empty command"));
        }
        Ok(())
    }

    pub fn has_extension(&self, extension: &str) -> bool {
        self.server.extensions.iter().any(|e| e == extension)
    }
}

impl ServerConfig {
    pub fn resolve_root_dir(&self, base: &Path) -> Result<PathBuf> {
        let path = if self.root_dir.is_absolute() {
            self.root_dir.clone()
        } else {
            base.join(&self.root_dir)
        };
        path.canonicalize()
            .with_context(|| format!("failed to resolve root directory: {}", path.display()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_valid_config() {
        let json = r#"{
            "server": {
                "extensions": ["js", "ts"],
                "command": ["typescript-language-server", "--stdio"],
                "rootDir": "."
            }
        }"#;
        let config: Config = serde_json::from_str(json).unwrap();
        assert_eq!(config.server.extensions, vec!["js", "ts"]);
    }

    #[test]
    fn reject_empty_extensions() {
        let json = r#"{
            "server": {
                "extensions": [],
                "command": ["server1"],
                "rootDir": "."
            }
        }"#;
        let config: Config = serde_json::from_str(json).unwrap();
        assert!(config.validate().is_err());
    }
}
