//! Utility functions for URI and file path handling.
//!
//! This module provides common utilities for working with file URIs,
//! extracting file extensions, and converting between URIs and paths.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result, anyhow};
use url::Url;

/// Extracts the file extension from a file:// URI.
///
/// Returns `None` if the URI has no extension or cannot be parsed as a path.
///
/// # Examples
///
/// ```
/// # use pathfinder::utils::extension_from_uri;
/// assert_eq!(extension_from_uri("file:///path/to/file.rs"), Some("rs".to_string()));
/// assert_eq!(extension_from_uri("file:///path/to/file"), None);
/// ```
pub fn extension_from_uri(uri: &str) -> Option<String> {
    let path = Path::new(uri);
    path.extension()
        .and_then(|ext| ext.to_str())
        .map(|s| s.to_string())
}

/// Converts a file:// URI to a PathBuf.
///
/// This function validates that the URI is a valid file:// URI and that
/// the resulting path exists on the filesystem.
///
/// # Errors
///
/// Returns an error if:
/// - The URI cannot be parsed
/// - The URI is not a file:// scheme
/// - The resolved path does not exist
pub fn uri_to_path(uri: &str) -> Result<PathBuf> {
    let parsed = Url::parse(uri).context("invalid URI")?;
    let path = parsed
        .to_file_path()
        .map_err(|_| anyhow!("only file:// URIs are supported"))?;

    // Validate the path exists
    if !path.exists() {
        return Err(anyhow!("document path does not exist: {}", path.display()));
    }

    Ok(path)
}

/// Determines the LSP language identifier for a given file path.
///
/// Maps common file extensions to their corresponding LSP language identifiers.
/// Returns the extension itself if no specific mapping is found, or "plaintext"
/// for files with no extension.
///
/// # Why LSP needs languageId
///
/// Even though pathfinder routes files to LSP servers by extension, the LSP
/// protocol requires a `languageId` field in `textDocument/didOpen` for several reasons:
///
/// 1. **Protocol requirement**: The LSP spec mandates this field in TextDocumentItem
/// 2. **Polyglot servers**: Some servers handle multiple languages (e.g., typescript-language-server
///    handles "javascript", "javascriptreact", "typescript", "typescriptreact")
/// 3. **Parser selection**: The languageId tells the server which parser to use
///    (e.g., TSX parser vs TS parser for different syntax rules)
/// 4. **No filesystem access**: LSP servers don't have direct filesystem access, so they
///    can't infer language from file extensions reliably
/// 5. **Ambiguous extensions**: Some extensions can represent multiple languages
///    (.h = C/C++/Objective-C, .m = Objective-C/MATLAB)
///
/// For single-language servers like rust-analyzer, this may seem redundant, but the
/// protocol is designed to be general-purpose and some servers validate this field.
pub fn language_id_for_path(path: &Path) -> &str {
    match path.extension().and_then(|ext| ext.to_str()).unwrap_or("") {
        "rs" => "rust",
        "go" => "go",
        "py" => "python",
        "ts" => "typescript",
        "tsx" => "typescriptreact",
        "js" => "javascript",
        "jsx" => "javascriptreact",
        "json" => "json",
        "toml" => "toml",
        "yaml" | "yml" => "yaml",
        "md" => "markdown",
        "" => "plaintext",
        other => other,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extension_from_uri() {
        assert_eq!(
            extension_from_uri("file:///path/to/file.rs"),
            Some("rs".to_string())
        );
        assert_eq!(
            extension_from_uri("file:///path/to/file.py"),
            Some("py".to_string())
        );
        assert_eq!(extension_from_uri("file:///path/to/file"), None);
    }

    #[test]
    fn test_language_id_for_path() {
        assert_eq!(language_id_for_path(Path::new("file.rs")), "rust");
        assert_eq!(language_id_for_path(Path::new("file.py")), "python");
        assert_eq!(language_id_for_path(Path::new("file.ts")), "typescript");
        assert_eq!(
            language_id_for_path(Path::new("file.tsx")),
            "typescriptreact"
        );
        assert_eq!(language_id_for_path(Path::new("file.js")), "javascript");
        assert_eq!(
            language_id_for_path(Path::new("file.jsx")),
            "javascriptreact"
        );
        assert_eq!(language_id_for_path(Path::new("file.unknown")), "unknown");
        assert_eq!(language_id_for_path(Path::new("file")), "plaintext");
    }
}
