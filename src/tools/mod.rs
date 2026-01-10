//! MCP tool implementations.
//!
//! This module provides the implementation of MCP tools that wrap LSP functionality.
//! Currently supports jump-to-definition, with room for expansion to other LSP features.

pub mod definition;

pub use definition::{DefinitionRequest, DefinitionResponse, DefinitionTool};
