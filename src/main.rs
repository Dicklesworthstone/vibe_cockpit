//! Vibe Cockpit - Agent fleet monitoring and orchestration
//!
//! This is the main entry point for the `vc` binary.
//! Uses Asupersync as the primary async runtime with a Tokio compat bridge
//! for downstream crates that still call Tokio APIs directly (Phases 2a–2e
//! will migrate those crates individually).

use anyhow::Result;
use asupersync::Cx;
use asupersync::runtime::{Runtime, RuntimeBuilder};
use asupersync_tokio_compat::runtime::with_tokio_context;
use clap::{CommandFactory, FromArgMatches};
use tracing_subscriber::{EnvFilter, fmt, prelude::*};
use vc_cli::Cli;

fn main() -> Result<()> {
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

    // ── Asupersync runtime (primary) ─────────────────────────────────────
    tracing::info!("initializing Asupersync runtime");
    let asupersync_rt = build_asupersync_runtime()?;
    tracing::debug!("Asupersync runtime created");

    // Establish root Cx capability token for structured concurrency.
    // Tokio-locked code is still quarantined behind a compat runtime, but the
    // request-scoped capability token is the authoritative execution context.
    let root_cx = Cx::for_request();
    tracing::debug!("root Cx established (region={:?})", root_cx.region_id());

    // ── Tokio compat runtime (secondary) ─────────────────────────────────
    // Required while downstream crates still call tokio APIs directly.
    // Will be removed once Phases 2a–2e migrate all crate-level tokio usage.
    tracing::debug!("initializing Tokio compat runtime");
    let tokio_rt = build_tokio_compat_runtime()?;
    let _tokio_guard = tokio_rt.enter();
    tracing::debug!("Tokio compat runtime entered");
    tracing::info!("Tokio compat bridge ready");

    // ── Run the CLI ──────────────────────────────────────────────────────
    tracing::info!("starting CLI execution");
    let cli_cx = root_cx.clone();
    let cli_result = asupersync_rt.block_on(async {
        with_tokio_context(&root_cx, || async move { cli.run_with_cx(&cli_cx).await }).await
    });
    let Some(cli_result) = cli_result else {
        tracing::warn!("CLI execution was cancelled before completion");
        anyhow::bail!("CLI execution was cancelled before completion");
    };
    tracing::debug!("CLI future completed inside runtime bridge");
    cli_result?;
    tracing::info!("CLI execution completed successfully");

    tracing::info!("graceful shutdown complete");
    Ok(())
}

fn build_asupersync_runtime() -> Result<Runtime> {
    tracing::debug!("building Asupersync runtime via RuntimeBuilder::new()");
    RuntimeBuilder::new().build().map_err(anyhow::Error::from)
}

fn build_tokio_compat_runtime() -> Result<tokio::runtime::Runtime> {
    tracing::debug!("building Tokio compat runtime (multi-thread, enable_all)");
    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .map_err(anyhow::Error::from)
}

fn build_version() -> String {
    let pkg = env!("CARGO_PKG_VERSION");
    let sha = env!("VERGEN_GIT_SHA");
    let ts = env!("VERGEN_BUILD_TIMESTAMP");
    format!("{pkg} ({sha}; built {ts})")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn runtime_bootstrap_runs_tokio_compat_work() {
        let asupersync_rt = build_asupersync_runtime().expect("build asupersync runtime");
        let tokio_rt = build_tokio_compat_runtime().expect("build tokio compat runtime");
        let _tokio_guard = tokio_rt.enter();
        let root_cx = Cx::for_request();

        let result = asupersync_rt.block_on(async {
            with_tokio_context(&root_cx, || async {
                let task = tokio::spawn(async {
                    tokio::time::sleep(Duration::from_millis(1)).await;
                    42_u8
                });
                task.await.expect("tokio task should complete")
            })
            .await
        });

        assert_eq!(result, Some(42));
    }

    #[test]
    fn build_version_includes_package_sha_and_timestamp() {
        let version = build_version();
        assert!(version.contains(env!("CARGO_PKG_VERSION")));
        assert!(version.contains(env!("VERGEN_GIT_SHA")));
        assert!(version.contains("built"));
    }
}
