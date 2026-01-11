use std::env;
use std::path::PathBuf;

use anyhow::{Context, Result, anyhow};
use tempfile::{TempDir, tempdir};
use tokio::runtime::Runtime;
use tokio::time::{Duration, sleep};
use url::Url;
use which::which;

use pathfinder::config::{Config, ServerConfig};
use pathfinder::documents::DocumentManager;
use pathfinder::lsp_bridge::LspBridge;
use pathfinder::tools::{DefinitionRequest, DefinitionResponse, DefinitionTool};

const CARGO_TOML: &str = r#"[package]
name = "pathfinder_rust_fixture"
version = "0.1.0"
edition = "2021"

[dependencies]
"#;

const MAIN_RS: &str = r#"fn main() {
    let sum = add(1, 2);
    println!("{}", sum);
}

fn add(a: i32, b: i32) -> i32 {
    a + b
}
"#;

#[test]
fn definition_via_rust_analyzer() -> Result<()> {
    if env::var("RUN_LSP_TESTS").is_err() {
        eprintln!("skipping rust-analyzer integration test (set RUN_LSP_TESTS=1)");
        return Ok(());
    }

    let rust_analyzer = which("rust-analyzer").context("rust-analyzer binary not found on PATH")?;
    let (_tempdir, workspace) = setup_workspace()?;

    let config = Config {
        server: ServerConfig {
            extensions: vec!["rs".to_string()],
            command: vec![rust_analyzer.display().to_string()],
            root_dir: PathBuf::from("."),
        },
    };

    let runtime = Runtime::new()?;
    runtime.block_on(async move {
        // Initialize LSP bridge
        let resolved_workspace = config.server.resolve_root_dir(&workspace)?;
        let mut lsp = LspBridge::new_with_command(
            &config.server.command[0],
            config.server.command[1..].to_vec(),
            resolved_workspace,
        )
        .await?;
        lsp.initialize().await?;

        let mut documents = DocumentManager::new();
        let tool = DefinitionTool::new();

        let main_uri = file_uri(workspace.join("src/main.rs"));
        documents.ensure_open(&mut lsp, &main_uri).await?;

        let response = wait_for_definition(
            &tool,
            &mut lsp,
            DefinitionRequest {
                uri: main_uri.clone(),
                line: 1,
                character: 16,
            },
        )
        .await?;

        let target = response
            .targets
            .first()
            .expect("definition response should contain at least one target");
        assert_eq!(target.uri, main_uri, "definition uri mismatch");
        assert_eq!(
            target.range.start_line, 5,
            "expected function definition on line 6 (0-based 5)"
        );
        assert!(
            target.range.start_character <= 4,
            "expected definition column near start, got {}",
            target.range.start_character
        );
        assert!(
            target.range.end_line >= target.range.start_line,
            "range end line must be >= start line"
        );

        documents.close_all(&mut lsp).await.ok();
        lsp.shutdown().await.ok();
        Ok::<(), anyhow::Error>(())
    })?;

    Ok(())
}

async fn wait_for_definition(
    tool: &DefinitionTool,
    lsp: &mut LspBridge,
    request: DefinitionRequest,
) -> Result<DefinitionResponse> {
    let mut attempts = 0;
    loop {
        attempts += 1;
        let response = tool.execute(lsp, request.clone()).await?;
        if !response.targets.is_empty() {
            return Ok(response);
        }
        if attempts >= 10 {
            return Err(anyhow!("definition never returned targets"));
        }
        sleep(Duration::from_millis(200)).await;
    }
}

fn setup_workspace() -> Result<(TempDir, PathBuf)> {
    let dir = tempdir()?;
    let path = dir.path().to_path_buf();
    std::fs::write(path.join("Cargo.toml"), CARGO_TOML)?;
    let src_dir = path.join("src");
    std::fs::create_dir_all(&src_dir)?;
    std::fs::write(src_dir.join("main.rs"), MAIN_RS)?;
    Ok((dir, path))
}

fn file_uri(path: PathBuf) -> String {
    Url::from_file_path(&path)
        .expect("workspace paths must be valid file URIs")
        .to_string()
}
