//! Document synchronization management.
//!
//! This module tracks open documents and ensures they are synchronized with the
//! LSP server. It handles didOpen, didChange, and didClose notifications,
//! and manages document versioning based on file modification times.

use std::collections::HashMap;
use std::time::SystemTime;

use anyhow::{Context, Result};
use serde_json::json;
use tokio::fs;

use crate::lsp_bridge::LspBridge;
use crate::utils::{language_id_for_path, uri_to_path};

#[derive(Debug)]
struct DocumentState {
    version: i32,
    mtime: SystemTime,
}

#[derive(Debug, Default)]
pub struct DocumentManager {
    open: HashMap<String, DocumentState>,
}

impl DocumentManager {
    pub fn new() -> Self {
        Self::default()
    }

    /// Ensures a document is opened and synchronized with the LSP server.
    ///
    /// This method:
    /// 1. Checks if the document is already open and up-to-date
    /// 2. Sends didOpen if the document is new
    /// 3. Sends didChange if the file has been modified since last sync
    /// 4. Skips sync if the document is already up-to-date
    pub async fn ensure_open(&mut self, lsp: &mut LspBridge, uri: &str) -> Result<()> {
        // Get file metadata to check modification time
        let path = uri_to_path(uri)?;
        let metadata = fs::metadata(&path)
            .await
            .with_context(|| format!("failed to read metadata for {}", path.display()))?;
        let modified = metadata.modified().unwrap_or(SystemTime::UNIX_EPOCH);

        match self.open.get(uri) {
            // Document is already open and unchanged - no action needed
            Some(state) if !is_newer(modified, state.mtime)? => {
                tracing::trace!("Document already synchronized: {}", uri);
                return Ok(());
            }
            // Document is open but has been modified - send didChange
            Some(state) => {
                tracing::debug!("Document modified, sending didChange: {}", uri);
                let text = fs::read_to_string(&path)
                    .await
                    .with_context(|| format!("failed to read {}", path.display()))?;
                let next_version = state.version + 1;
                self.send_did_change(lsp, uri, next_version, &text).await?;
                self.open.insert(
                    uri.to_string(),
                    DocumentState {
                        version: next_version,
                        mtime: modified,
                    },
                );
            }
            // Document is not yet open - send didOpen
            None => {
                tracing::debug!("Opening new document: {}", uri);
                let text = fs::read_to_string(&path)
                    .await
                    .with_context(|| format!("failed to read {}", path.display()))?;

                // The LSP protocol requires a languageId in textDocument/didOpen.
                // This tells the server which parser to use and enables proper handling
                // for polyglot servers (e.g., typescript-language-server needs to know
                // whether to parse as "typescript" or "typescriptreact").
                let language_id = language_id_for_path(&path);
                let version = 1;
                self.send_did_open(lsp, uri, language_id, version, &text)
                    .await?;
                self.open.insert(
                    uri.to_string(),
                    DocumentState {
                        version,
                        mtime: modified,
                    },
                );
            }
        }
        Ok(())
    }

    pub async fn close_all(&mut self, lsp: &mut LspBridge) -> Result<()> {
        for uri in self.open.keys().cloned().collect::<Vec<_>>() {
            let _ = self.send_did_close(lsp, &uri).await;
        }
        self.open.clear();
        Ok(())
    }

    async fn send_did_open(
        &mut self,
        lsp: &mut LspBridge,
        uri: &str,
        language_id: &str,
        version: i32,
        text: &str,
    ) -> Result<()> {
        let params = json!({
            "textDocument": {
                "uri": uri,
                "languageId": language_id,
                "version": version,
                "text": text,
            }
        });
        lsp.notify("textDocument/didOpen", params).await
    }

    async fn send_did_change(
        &mut self,
        lsp: &mut LspBridge,
        uri: &str,
        version: i32,
        text: &str,
    ) -> Result<()> {
        let params = json!({
            "textDocument": {
                "uri": uri,
                "version": version,
            },
            "contentChanges": [{
                "text": text
            }]
        });
        lsp.notify("textDocument/didChange", params).await
    }

    async fn send_did_close(&mut self, lsp: &mut LspBridge, uri: &str) -> Result<()> {
        let params = json!({
            "textDocument": {
                "uri": uri
            }
        });
        lsp.notify("textDocument/didClose", params).await
    }
}

/// Checks if timestamp `a` is newer than timestamp `b`.
fn is_newer(a: SystemTime, b: SystemTime) -> Result<bool> {
    Ok(a.duration_since(b)
        .map(|d| d.as_nanos() > 0)
        .unwrap_or(false))
}
