use anyhow::{Context, Result, anyhow};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};
use tokio::time::{Duration, sleep};

use crate::lsp_bridge::LspBridge;

const MAX_RETRIES: u32 = 3;
const RETRY_DELAY_MS: u64 = 150;

#[derive(Debug, Deserialize, Clone, schemars::JsonSchema)]
pub struct DefinitionRequest {
    /// file:// URI of the document
    pub uri: String,
    /// Zero-based line index
    pub line: u32,
    /// Zero-based character index
    pub character: u32,
}

#[derive(Debug, Serialize, Clone, Default)]
pub struct DefinitionResponse {
    pub targets: Vec<DefinitionTarget>,
}

#[derive(Debug, Serialize, Clone)]
pub struct DefinitionTarget {
    pub uri: String,
    pub range: TextRange,
}

#[derive(Debug, Serialize, Clone)]
pub struct TextRange {
    pub start_line: u32,
    pub start_character: u32,
    pub end_line: u32,
    pub end_character: u32,
}

#[derive(Debug, Default, Clone, Copy)]
pub struct DefinitionTool;

impl DefinitionTool {
    pub fn new() -> Self {
        Self
    }

    pub fn description() -> &'static str {
        "Return LSP-backed jump-to-definition targets for a given URI and position"
    }

    pub fn schema() -> Value {
        json!({
            "type": "object",
            "properties": {
                "uri": {
                    "type": "string",
                    "description": "file:// URI of the document"
                },
                "line": {
                    "type": "integer",
                    "description": "Zero-based line index"
                },
                "character": {
                    "type": "integer",
                    "description": "Zero-based character index"
                }
            },
            "required": ["uri", "line", "character"]
        })
    }

    pub fn descriptor() -> Value {
        json!({
            "name": "definition",
            "description": Self::description(),
            "inputSchema": Self::schema(),
        })
    }

    pub async fn execute(
        &self,
        lsp: &mut LspBridge,
        request: DefinitionRequest,
    ) -> Result<DefinitionResponse> {
        let params = json!({
            "textDocument": { "uri": request.uri },
            "position": { "line": request.line, "character": request.character },
        });

        // Retry logic for empty results
        // LSP servers sometimes return empty initially during indexing
        for attempt in 1..=MAX_RETRIES {
            let raw = lsp
                .request("textDocument/definition", params.clone())
                .await
                .context("LSP definition request failed")?;
            let targets = normalize_targets(&raw)?;

            if !targets.is_empty() {
                if attempt > 1 {
                    tracing::debug!(attempt, uri = %request.uri, "Definition succeeded after retry");
                }
                return Ok(DefinitionResponse { targets });
            }

            // Empty result - retry if we have attempts left
            if attempt < MAX_RETRIES {
                tracing::debug!(attempt, uri = %request.uri, "Definition empty, retrying...");
                sleep(Duration::from_millis(RETRY_DELAY_MS)).await;
            }
        }

        // All retries returned empty - return empty result
        Ok(DefinitionResponse { targets: vec![] })
    }
}

/// Normalizes LSP definition responses into a consistent format.
///
/// LSP servers can return definitions in three formats:
/// - null (no definition found)
/// - Location (single result)
/// - Location[] (multiple results)
/// - LocationLink[] (alternative format with more info)
///
/// This function converts all formats to a Vec<DefinitionTarget>.
fn normalize_targets(value: &Value) -> Result<Vec<DefinitionTarget>> {
    match value {
        Value::Null => Ok(vec![]),
        Value::Array(entries) => entries.iter().map(convert_location).collect(),
        Value::Object(_) => Ok(vec![convert_location(value)?]),
        other => Err(anyhow!("unexpected definition response format: {other:?}")),
    }
}

/// Converts a single location entry to a DefinitionTarget.
///
/// Handles both Location and LocationLink formats:
/// - Location: { uri, range }
/// - LocationLink: { targetUri, targetRange, ... }
fn convert_location(value: &Value) -> Result<DefinitionTarget> {
    let object = value
        .as_object()
        .ok_or_else(|| anyhow!("definition entry must be an object"))?;

    // Try Location format first, then LocationLink format
    if object.contains_key("uri") {
        convert_standard_location(object)
    } else if object.contains_key("targetUri") {
        convert_location_link(object)
    } else {
        Err(anyhow!(
            "definition entry missing required fields (expected 'uri' or 'targetUri'): {object:?}"
        ))
    }
}

fn convert_standard_location(object: &Map<String, Value>) -> Result<DefinitionTarget> {
    let uri = object
        .get("uri")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow!("location.uri must be a string"))?;
    let range_value = object
        .get("range")
        .ok_or_else(|| anyhow!("location.range missing"))?;
    let range = parse_range(range_value)?;
    Ok(DefinitionTarget {
        uri: uri.to_string(),
        range,
    })
}

fn convert_location_link(object: &Map<String, Value>) -> Result<DefinitionTarget> {
    let uri = object
        .get("targetUri")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow!("locationLink.targetUri must be a string"))?;
    let range_value = object
        .get("targetRange")
        .ok_or_else(|| anyhow!("locationLink.targetRange missing"))?;
    let range = parse_range(range_value)?;
    Ok(DefinitionTarget {
        uri: uri.to_string(),
        range,
    })
}

fn parse_range(value: &Value) -> Result<TextRange> {
    let obj = value
        .as_object()
        .ok_or_else(|| anyhow!("range must be an object"))?;
    let start = obj
        .get("start")
        .ok_or_else(|| anyhow!("range.start missing"))?;
    let end = obj.get("end").ok_or_else(|| anyhow!("range.end missing"))?;

    Ok(TextRange {
        start_line: get_coord(start, "line", "start")?,
        start_character: get_coord(start, "character", "start")?,
        end_line: get_coord(end, "line", "end")?,
        end_character: get_coord(end, "character", "end")?,
    })
}

fn get_coord(value: &Value, coord: &str, position_label: &str) -> Result<u32> {
    value
        .as_object()
        .and_then(|obj| obj.get(coord))
        .and_then(|num| num.as_u64())
        .map(|v| v as u32)
        .ok_or_else(|| {
            anyhow!(
                "range.{}.{} must be an unsigned integer",
                position_label,
                coord
            )
        })
}
