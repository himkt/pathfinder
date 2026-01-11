use std::env;
use std::path::PathBuf;

use anyhow::{Result, anyhow};
use tracing_subscriber::{EnvFilter, fmt};

use rmcp::{ServiceExt, transport::stdio};

use clap::Parser;

use pathfinder::args::Cli;
use pathfinder::config::Config;
use pathfinder::service::PathfinderService;

#[tokio::main]
async fn main() -> Result<()> {
    init_tracing()?;

    let cli = Cli::parse();
    let workspace_arg = cli.workspace.clone();
    let server_specs = cli.to_server_specs()?;

    // Extract the single server spec (CLI always produces one spec)
    let server_spec = server_specs
        .into_iter()
        .next()
        .ok_or_else(|| anyhow!("no server specification provided"))?;

    let config = Config::from_server_spec(server_spec)?;

    let workspace_base = if let Some(ws) = workspace_arg {
        canonical_path(ws)?
    } else {
        env::current_dir().map_err(|err| anyhow!("failed to get current directory: {err}"))?
    };

    tracing::info!(
        workspace_base = %workspace_base.display(),
        extensions = ?config.server.extensions,
        command = ?config.server.command,
        "Starting pathfinder"
    );

    let service = PathfinderService::new(config, workspace_base).await?;
    let server = service.serve(stdio()).await?;
    server.waiting().await?;
    Ok(())
}

fn init_tracing() -> Result<()> {
    let env_filter = EnvFilter::try_from_default_env()
        .or_else(|_| EnvFilter::try_new(env::var("LOG_LEVEL").unwrap_or_else(|_| "info".into())))?;

    fmt::Subscriber::builder()
        .with_env_filter(env_filter)
        .with_target(false)
        .with_writer(std::io::stderr)
        .init();
    Ok(())
}

fn canonical_path(path: PathBuf) -> Result<PathBuf> {
    let abs = if path.is_absolute() {
        path
    } else {
        env::current_dir()
            .map_err(|err| anyhow!("failed to read current directory: {err}"))?
            .join(path)
    };
    abs.canonicalize()
        .map_err(|err| anyhow!("failed to canonicalize path: {err}"))
}
