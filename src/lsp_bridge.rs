//! LSP bridge implementation.
//!
//! This module provides the `LspBridge` type which manages a single LSP server process.
//! It handles process spawning, JSON-RPC communication, initialization handshake,
//! and graceful shutdown. Each bridge maintains its own request ID sequence and
//! enforces a 15-second timeout on all requests.

use std::path::PathBuf;
use std::process::Stdio;
use std::time::Duration;

use anyhow::{Context, Result, anyhow};
use serde_json::{Value, json};
use tokio::process::{Child, ChildStdin, ChildStdout, Command};
use tokio::time::timeout;
use url::Url;

use crate::transport::FramedTransport;

pub struct LspBridge {
    workspace: PathBuf,
    child: Child,
    transport: FramedTransport<ChildStdout, ChildStdin>,
    next_request_id: i64,
}

const REQUEST_TIMEOUT: Duration = Duration::from_secs(15);

impl LspBridge {
    pub async fn new_with_command(
        command: &str,
        args: Vec<String>,
        workspace: PathBuf,
    ) -> Result<Self> {
        tracing::debug!(command = %command, ?args, "Spawning LSP child process");
        let mut cmd = Command::new(command);
        if !args.is_empty() {
            cmd.args(&args);
        }
        cmd.current_dir(&workspace);
        cmd.stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit());

        let mut child = cmd
            .spawn()
            .context("failed to spawn language server process")?;
        let stdout = child
            .stdout
            .take()
            .context("language server stdout not captured")?;
        let stdin = child
            .stdin
            .take()
            .context("language server stdin not captured")?;

        let transport = FramedTransport::new(stdout, stdin);

        Ok(Self {
            workspace,
            child,
            transport,
            next_request_id: 1,
        })
    }

    pub async fn initialize(&mut self) -> Result<()> {
        let root_uri = Url::from_directory_path(&self.workspace)
            .map_err(|_| anyhow!("workspace path cannot be expressed as file URI"))?;
        let workspace_name = self
            .workspace
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("workspace");

        let params = json!({
            "processId": std::process::id(),
            "rootUri": root_uri,
            "rootPath": self.workspace,
            "capabilities": serde_json::Map::new(),
            "workspaceFolders": [{
                "name": workspace_name,
                "uri": root_uri,
            }]
        });

        let _ = self.request("initialize", params).await?;
        self.notify("initialized", json!({})).await?;
        Ok(())
    }

    /// Sends a JSON-RPC request to the LSP server and waits for the response.
    ///
    /// This method handles the request-response cycle, including:
    /// - Assigning a unique request ID
    /// - Writing the request to the LSP server
    /// - Waiting for and filtering the matching response
    /// - Discarding unrelated notifications during the wait
    pub async fn request(&mut self, method: &str, params: Value) -> Result<Value> {
        let id = self.next_request_id;
        self.next_request_id += 1;
        let payload = json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": method,
            "params": params,
        });
        self.transport.write(&payload).await?;

        // Wait for the response, filtering out unrelated messages
        loop {
            let read = timeout(REQUEST_TIMEOUT, self.transport.read()).await;
            let message = match read {
                Ok(inner) => inner?,
                Err(_) => {
                    return Err(anyhow!(
                        "timed out after {:?} waiting for LSP response to '{}'",
                        REQUEST_TIMEOUT,
                        method
                    ));
                }
            };

            match message {
                Some(Value::Object(obj)) => {
                    // Check if this is a response (has an "id" field)
                    if let Some(response_id) = obj.get("id") {
                        // Skip responses for different requests (can happen with concurrent requests)
                        if !matches_id(response_id, id) {
                            tracing::trace!("Skipping response for different id: {response_id:?}");
                            continue;
                        }

                        // Return successful result
                        if let Some(result) = obj.get("result") {
                            return Ok(result.clone());
                        }

                        // Return error if present
                        if let Some(error) = obj.get("error") {
                            return Err(anyhow!("LSP error for '{}': {error:?}", method));
                        }

                        // Invalid response format
                        return Err(anyhow!(
                            "invalid LSP response for '{}': missing both result and error fields",
                            method
                        ));
                    }

                    // Discard notifications (messages without "id")
                    tracing::trace!("discarding notification: {obj:?}");
                }
                Some(other) => {
                    tracing::warn!("received unexpected non-object message: {other:?}");
                }
                None => {
                    return Err(anyhow!(
                        "LSP server terminated unexpectedly before responding to '{}'",
                        method
                    ));
                }
            }
        }
    }

    pub async fn notify(&mut self, method: &str, params: Value) -> Result<()> {
        let payload = json!({
            "jsonrpc": "2.0",
            "method": method,
            "params": params,
        });
        self.transport.write(&payload).await
    }

    /// Gracefully shuts down the LSP server process.
    ///
    /// Follows the LSP shutdown protocol:
    /// 1. Send "shutdown" request and wait for response
    /// 2. Send "exit" notification
    /// 3. Wait for process to terminate
    /// 4. Force kill if any step fails or times out
    pub async fn shutdown(mut self) -> Result<()> {
        tracing::debug!("Initiating graceful LSP shutdown");

        // Step 1: Send shutdown request (LSP protocol requirement)
        if let Err(err) = self.request("shutdown", Value::Null).await {
            tracing::warn!(?err, "LSP shutdown request failed; forcing kill");
            self.child
                .kill()
                .await
                .context("failed to kill LSP child after shutdown failure")?;
            return Ok(());
        }

        // Step 2: Send exit notification (LSP protocol requirement)
        if let Err(err) = self.notify("exit", Value::Null).await {
            tracing::warn!(
                ?err,
                "Failed to send LSP exit notification; will still wait for process"
            );
        }

        // Step 3: Wait for process to exit, with timeout
        match timeout(REQUEST_TIMEOUT, self.child.wait()).await {
            Ok(Ok(status)) => {
                tracing::debug!(?status, "LSP server exited cleanly");
            }
            Ok(Err(err)) => {
                tracing::warn!(?err, "Error waiting for LSP process; forcing kill");
                self.child
                    .kill()
                    .await
                    .context("failed to kill unresponsive LSP process")?;
            }
            Err(_) => {
                tracing::warn!(
                    timeout = ?REQUEST_TIMEOUT,
                    "Timed out waiting for LSP to exit; forcing kill"
                );
                self.child
                    .kill()
                    .await
                    .context("failed to kill timed-out LSP process")?;
            }
        }
        Ok(())
    }
}

/// Checks if a JSON value matches the expected request ID.
///
/// LSP allows IDs to be either numbers or strings, so we handle both.
fn matches_id(candidate: &Value, id: i64) -> bool {
    candidate.as_i64() == Some(id) || candidate.as_str() == Some(&id.to_string())
}
