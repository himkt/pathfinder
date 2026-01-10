//! JSON-RPC transport layer for LSP communication.
//!
//! This module provides a Content-Length framed transport implementation that handles
//! reading and writing JSON-RPC messages over stdio streams. The transport is used by
//! LSP bridges to communicate with language server processes.

use std::collections::HashMap;

use anyhow::{Context, Result, anyhow};
use serde_json::Value;
use tokio::io::{AsyncBufReadExt, AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt, BufReader};
use tracing::warn;

/// Content-Length framed JSON-RPC transport used for LSP streams.
pub struct FramedTransport<R, W> {
    reader: BufReader<R>,
    writer: W,
}

impl<R, W> FramedTransport<R, W>
where
    R: AsyncRead + Unpin,
    W: AsyncWrite + Unpin,
{
    pub fn new(reader: R, writer: W) -> Self {
        Self {
            reader: BufReader::new(reader),
            writer,
        }
    }

    /// Reads the next JSON-RPC payload. Returns Ok(None) on EOF.
    pub async fn read(&mut self) -> Result<Option<Value>> {
        let headers = match self.read_headers().await? {
            Some(h) => h,
            None => return Ok(None),
        };

        let length = headers
            .get("content-length")
            .ok_or_else(|| anyhow!("missing Content-Length header"))?
            .parse::<usize>()
            .context("could not parse Content-Length header as usize")?;

        let mut buf = vec![0u8; length];
        self.reader
            .read_exact(&mut buf)
            .await
            .context("failed to read JSON payload body")?;

        let value = serde_json::from_slice(&buf).context("invalid JSON in framed payload")?;
        Ok(Some(value))
    }

    /// Writes a JSON-RPC payload with Content-Length header.
    pub async fn write(&mut self, value: &Value) -> Result<()> {
        let body = serde_json::to_vec(value).context("failed to serialize JSON payload")?;
        let header = format!("Content-Length: {}\r\n\r\n", body.len());
        self.writer
            .write_all(header.as_bytes())
            .await
            .context("failed to write framed header")?;
        self.writer
            .write_all(&body)
            .await
            .context("failed to write framed body")?;
        self.writer
            .flush()
            .await
            .context("failed to flush writer")?;
        Ok(())
    }

    async fn read_headers(&mut self) -> Result<Option<HashMap<String, String>>> {
        let mut headers = HashMap::new();
        let mut line = String::new();
        loop {
            line.clear();
            let bytes = self
                .reader
                .read_line(&mut line)
                .await
                .context("failed to read header line")?;

            if bytes == 0 {
                if headers.is_empty() {
                    return Ok(None);
                }
                return Err(anyhow!("unexpected EOF while reading headers"));
            }

            let trimmed = line.trim_end_matches(['\r', '\n']);

            if trimmed.is_empty() {
                if headers.is_empty() {
                    continue;
                }
                break;
            }

            if let Some((name, value)) = trimmed.split_once(':') {
                headers.insert(name.trim().to_ascii_lowercase(), value.trim().to_string());
            } else {
                warn!("Ignoring non-header line from LSP: {}", trimmed);
            }
        }
        Ok(Some(headers))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use tokio::io::{self, DuplexStream};

    fn transport_pair() -> (
        FramedTransport<DuplexStream, DuplexStream>,
        FramedTransport<DuplexStream, DuplexStream>,
    ) {
        let (right_writer_stream, left_reader_stream) = io::duplex(1024);
        let (left_writer_stream, right_reader_stream) = io::duplex(1024);
        (
            FramedTransport::new(left_reader_stream, left_writer_stream),
            FramedTransport::new(right_reader_stream, right_writer_stream),
        )
    }

    #[tokio::test]
    async fn round_trip_json_payload() {
        let (mut left, mut right) = transport_pair();
        let payload = json!({"jsonrpc": "2.0", "id": 1, "method": "test"});
        left.write(&payload).await.unwrap();
        let read_back = right.read().await.unwrap().unwrap();
        assert_eq!(payload, read_back);
    }

    #[tokio::test]
    async fn eof_returns_none() {
        let (left, mut right) = transport_pair();
        drop(left);
        let next = right.read().await.unwrap();
        assert!(next.is_none());
    }
}
