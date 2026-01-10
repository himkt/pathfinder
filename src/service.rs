//! MCP service implementation for pathfinder.
//!
//! This module implements the Model Context Protocol (MCP) server that exposes
//! LSP functionality as MCP tools. It manages the LSP bridge and document synchronization,
//! and routes MCP tool calls to the LSP server.

use std::path::PathBuf;
use std::sync::Arc;

use anyhow::Result;
use rmcp::{
    ErrorData as McpError, RoleServer, ServerHandler,
    handler::server::{router::tool::ToolRouter, wrapper::Parameters},
    model::*,
    service::RequestContext,
    tool, tool_handler, tool_router,
};
use tokio::sync::Mutex;

use crate::config::Config;
use crate::documents::DocumentManager;
use crate::lsp_bridge::LspBridge;
use crate::tools::definition::{DefinitionRequest, DefinitionTool};

#[derive(Clone)]
pub struct PathfinderService {
    lsp: Arc<Mutex<LspBridge>>,
    documents: Arc<Mutex<DocumentManager>>,
    tool_router: ToolRouter<PathfinderService>,
}

#[tool_router]
impl PathfinderService {
    pub async fn new(config: Config, workspace_base: PathBuf) -> Result<Self> {
        // Initialize the LSP bridge
        let workspace = config.server.resolve_root_dir(&workspace_base)?;
        let command = &config.server.command[0];
        let args: Vec<String> = config.server.command[1..].to_vec();

        let mut lsp = LspBridge::new_with_command(command, args, workspace).await?;
        lsp.initialize().await?;

        let documents = DocumentManager::new();

        Ok(Self {
            lsp: Arc::new(Mutex::new(lsp)),
            documents: Arc::new(Mutex::new(documents)),
            tool_router: Self::tool_router(),
        })
    }

    /// Return LSP-backed jump-to-definition targets for a given URI and position
    #[tool(
        description = "Return LSP-backed jump-to-definition targets for a given URI and position"
    )]
    async fn definition(
        &self,
        Parameters(request): Parameters<DefinitionRequest>,
    ) -> Result<CallToolResult, McpError> {
        // Ensure document is open
        {
            let mut documents = self.documents.lock().await;
            let mut lsp = self.lsp.lock().await;
            if let Err(err) = documents.ensure_open(&mut lsp, &request.uri).await {
                tracing::warn!(?err, "Failed to sync document before definition call");
                return Ok(CallToolResult::error(vec![Content::text(format!(
                    "failed to prepare document: {err}"
                ))]));
            }
        }

        // Execute definition tool
        let tool = DefinitionTool::new();
        let mut lsp = self.lsp.lock().await;
        match tool.execute(&mut lsp, request).await {
            Ok(response) => {
                let json_value = serde_json::to_value(response).map_err(|e| {
                    McpError::internal_error(format!("serialization failed: {e}"), None)
                })?;
                let content = Content::json(json_value).map_err(|e| {
                    McpError::internal_error(format!("content creation failed: {e}"), None)
                })?;
                Ok(CallToolResult::success(vec![content]))
            }
            Err(err) => Ok(CallToolResult::error(vec![Content::text(format!(
                "definition failed: {err}"
            ))])),
        }
    }
}

#[tool_handler]
impl ServerHandler for PathfinderService {
    fn get_info(&self) -> ServerInfo {
        ServerInfo {
            protocol_version: ProtocolVersion::V_2024_11_05,
            capabilities: ServerCapabilities::builder()
                .enable_tools()
                .build(),
            server_info: Implementation::from_build_env(),
            instructions: Some("MCP server that bridges to Language Server Protocol (LSP) servers. Provides jump-to-definition and other LSP features.".to_string()),
        }
    }

    async fn initialize(
        &self,
        _request: InitializeRequestParam,
        _context: RequestContext<RoleServer>,
    ) -> Result<InitializeResult, McpError> {
        tracing::info!("MCP client connected and initialized");
        Ok(self.get_info())
    }
}
