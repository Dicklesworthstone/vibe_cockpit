//! Vibe Cockpit - Agent fleet monitoring and orchestration
//!
//! This is the main entry point for the `vc` binary.

use anyhow::Result;
use clap::{CommandFactory, FromArgMatches};
use tracing_subscriber::{EnvFilter, fmt, prelude::*};
use vc_cli::Cli;

#[tokio::main]
async fn main() -> Result<()> {
    // Parse CLI arguments with build metadata in version output
    let mut cmd = Cli::command();
    let version: &'static str = Box::leak(build_version().into_boxed_str());
    cmd = cmd.version(version);
    let matches = cmd.get_matches();
    let cli = Cli::from_arg_matches(&matches)?;

    // Set up logging based on verbosity
    let filter = if cli.verbose {
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("debug"))
    } else {
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"))
    };

    tracing_subscriber::registry()
        .with(fmt::layer())
        .with(filter)
        .init();

    // Run the CLI
    cli.run().await?;

    Ok(())
}

fn build_version() -> String {
    let pkg = env!("CARGO_PKG_VERSION");
    let sha = env!("VERGEN_GIT_SHA");
    let ts = env!("VERGEN_BUILD_TIMESTAMP");
    format!("{pkg} ({sha}; built {ts})")
}
