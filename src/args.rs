//! Command-line argument parsing using clap.

use std::path::PathBuf;

use anyhow::{Result, anyhow};
use clap::Parser;

/// MCP server that bridges to Language Server Protocol servers
#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
#[command(after_help = "EXAMPLES:\n  \
    pathfinder -e py -s pyright-langserver -- --stdio\n  \
    pathfinder -e py -e pyi -s uv run pyright -- --stdio\n  \
    pathfinder -e rs -s rust-analyzer -w /path/to/project")]
pub struct Cli {
    /// File extensions to handle (can be specified multiple times)
    ///
    /// Examples: py, rs, js, ts
    #[arg(short, long, value_name = "EXT", action = clap::ArgAction::Append, required = true)]
    pub extension: Vec<String>,

    /// LSP server command and arguments
    ///
    /// Everything after --server is passed to the LSP server.
    /// Use -- to clearly separate server flags: --server cmd -- --flag
    #[arg(short, long, value_name = "CMD", num_args = 1.., required = true, allow_hyphen_values = true)]
    pub server: Vec<String>,

    /// Workspace base directory (defaults to current directory)
    #[arg(short, long, value_name = "PATH")]
    pub workspace: Option<PathBuf>,
}

#[derive(Debug)]
pub struct ServerSpec {
    pub extensions: Vec<String>,
    pub command: Vec<String>,
}

impl Cli {
    /// Convert CLI args to server specifications
    pub fn to_server_specs(self) -> Result<Vec<ServerSpec>> {
        if self.extension.is_empty() {
            return Err(anyhow!("at least one --extension must be specified"));
        }

        if self.server.is_empty() {
            return Err(anyhow!("--server command cannot be empty"));
        }

        Ok(vec![ServerSpec {
            extensions: self.extension,
            command: self.server,
        }])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse_args(args: &[&str]) -> Result<Cli> {
        let args: Vec<String> = std::iter::once("pathfinder")
            .chain(args.iter().copied())
            .map(String::from)
            .collect();
        Ok(Cli::try_parse_from(args)?)
    }

    #[test]
    fn python_single_extension() {
        let cli = parse_args(&["-e", "py", "-s", "pyright-langserver", "--", "--stdio"]).unwrap();
        assert_eq!(cli.extension, vec!["py"]);
        assert_eq!(cli.server, vec!["pyright-langserver", "--", "--stdio"]);
    }

    #[test]
    fn python_multiple_extensions() {
        // Python and stub files use the same LSP server
        let cli = parse_args(&[
            "-e",
            "py",
            "-e",
            "pyi",
            "-s",
            "pyright-langserver",
            "--",
            "--stdio",
        ])
        .unwrap();
        assert_eq!(cli.extension, vec!["py", "pyi"]);
        assert_eq!(cli.server, vec!["pyright-langserver", "--", "--stdio"]);
    }

    #[test]
    fn rust_with_workspace() {
        let cli = parse_args(&["-w", "/tmp/myproject", "-e", "rs", "-s", "rust-analyzer"]).unwrap();
        assert_eq!(cli.extension, vec!["rs"]);
        assert_eq!(cli.server, vec!["rust-analyzer"]);
        assert_eq!(cli.workspace, Some(PathBuf::from("/tmp/myproject")));
    }

    #[test]
    fn typescript_long_form() {
        // Using long-form flags
        let cli = parse_args(&[
            "--extension",
            "ts",
            "--server",
            "typescript-language-server",
            "--",
            "--stdio",
        ])
        .unwrap();
        assert_eq!(cli.extension, vec!["ts"]);
        assert_eq!(
            cli.server,
            vec!["typescript-language-server", "--", "--stdio"]
        );
    }

    #[test]
    fn python_with_uv() {
        // Real example: using uv to run pyright
        let cli = parse_args(&[
            "-e",
            "py",
            "-s",
            "uv",
            "run",
            "pyright-langserver",
            "--",
            "--stdio",
        ])
        .unwrap();
        assert_eq!(cli.extension, vec!["py"]);
        assert_eq!(
            cli.server,
            vec!["uv", "run", "pyright-langserver", "--", "--stdio"]
        );
    }

    #[test]
    fn javascript_react() {
        // TypeScript server handles JSX files too
        let cli = parse_args(&[
            "-e",
            "jsx",
            "-s",
            "typescript-language-server",
            "--",
            "--stdio",
        ])
        .unwrap();
        assert_eq!(cli.extension, vec!["jsx"]);
        assert_eq!(
            cli.server,
            vec!["typescript-language-server", "--", "--stdio"]
        );
    }

    #[test]
    fn error_no_extension() {
        let result = parse_args(&["-s", "rust-analyzer"]);
        assert!(result.is_err());
    }

    #[test]
    fn error_no_server() {
        let result = parse_args(&["-e", "py"]);
        assert!(result.is_err());
    }
}
