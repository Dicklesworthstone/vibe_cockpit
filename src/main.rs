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

    // ── Tokio compat runtime (secondary) ─────────────────────────────────
    // Required while downstream crates still call tokio APIs directly.
    // Will be removed once Phases 2a–2e migrate all crate-level tokio usage.
    tracing::debug!("initializing Tokio compat runtime");
    let tokio_rt = build_tokio_compat_runtime()?;
    let _tokio_guard = tokio_rt.enter();
    tracing::debug!("Tokio compat runtime entered");
    tracing::info!("Tokio compat bridge ready");

    // ── Run the CLI ──────────────────────────────────────────────────────
    // The root Cx is taken from inside `block_on`, which installs an ambient
    // context backed by this runtime's drivers. Minting one outside the runtime
    // boundary would hand every caller an unrestricted capability set.
    tracing::info!("starting CLI execution");
    let cli_result = asupersync_rt.block_on(async {
        let root_cx =
            Cx::current().expect("Runtime::block_on installs an ambient Cx for the polled future");
        tracing::debug!("root Cx established (region={:?})", root_cx.region_id());
        let cli_cx = root_cx.clone();
        let drain_cleanup = matches!(&cli.command, vc_cli::Commands::MigrateDb { .. });
        run_cli_operation(&root_cx, drain_cleanup, || async move {
            cli.run_with_cx(&cli_cx).await
        })
        .await
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

async fn run_cli_operation<F, Fut, T>(cx: &Cx, drain_cleanup: bool, operation: F) -> Option<T>
where
    F: FnOnce() -> Fut,
    Fut: std::future::Future<Output = T>,
{
    if !drain_cleanup {
        return with_tokio_context(cx, operation).await;
    }
    if cx.is_cancel_requested() {
        return None;
    }
    // The migration checks cancellation around its borrowed copy work. Its
    // connection-owning future must remain driven through asynchronous close;
    // the compat adapter can stop polling after its first cancelled Pending.
    // Runtime::block_on keeps the same root Cx installed on every poll here.
    let result = operation().await;
    if cx.is_cancel_requested() {
        None
    } else {
        Some(result)
    }
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
        let result = asupersync_rt.block_on(async {
            let root_cx = Cx::current().expect("runtime-owned root context");
            with_tokio_context(&root_cx, || async {
                let current = Cx::current().expect("bridge installs the owning context");
                assert_eq!(current.region_id(), root_cx.region_id());
                assert_eq!(current.capabilities(), root_cx.capabilities());
                let task = tokio::spawn(async {
                    tokio::time::sleep(Duration::from_millis(1)).await;
                    42_u8
                });
                let value = task.await.expect("tokio task should complete");
                let output = asupersync::process::Command::new("sh")
                    .arg("-c")
                    .arg("printf cockpit-owner")
                    .stdout(asupersync::process::Stdio::Pipe)
                    .stderr(asupersync::process::Stdio::Pipe)
                    .kill_on_drop(true)
                    .spawn()
                    .expect("spawn native collector-style command")
                    .wait_with_output_async(&current)
                    .await
                    .expect("native command completes under owner context");
                assert!(output.status.success());
                assert_eq!(output.stdout, b"cockpit-owner");
                assert!(output.stderr.is_empty());
                value
            })
            .await
        });

        assert_eq!(result, Some(42));
    }

    #[test]
    fn runtime_bridge_preserves_restricted_owner_and_cancellation() {
        let asupersync_rt = build_asupersync_runtime().expect("build asupersync runtime");
        let tokio_rt = build_tokio_compat_runtime().expect("build tokio compat runtime");
        let _tokio_guard = tokio_rt.enter();
        let budget = asupersync::Budget::new().with_poll_quota(8);
        let owner = asupersync_rt.request_cx_with_budget(budget);

        asupersync_rt.block_on(async {
            let parent = Cx::current().expect("runtime-owned parent");
            let captured = {
                let restricted = owner.restrict::<asupersync::cx::cap::None>();
                let _guard = restricted.set_current_restricted();
                Cx::current().expect("restricted ambient context")
            };
            let caps = captured.capabilities();
            assert!(!caps.spawn && !caps.time && !caps.entropy && !caps.io && !caps.remote);
            let result = with_tokio_context(&captured, || async {
                assert_eq!(Cx::current().expect("first poll").capabilities(), caps);
                tokio::task::yield_now().await;
                let current = Cx::current().expect("context restored on later poll");
                assert_eq!(current.capabilities(), caps);
                assert_eq!(current.budget(), budget);
                current.checkpoint().expect("live owner checkpoint");
                7_u8
            })
            .await;
            assert_eq!(result, Some(7));

            owner.cancel_with(asupersync::CancelKind::User, Some("owner cancelled"));
            let factory_called = std::cell::Cell::new(false);
            let result = with_tokio_context(&captured, || {
                factory_called.set(true);
                async { 9_u8 }
            })
            .await;
            assert_eq!(result, None);
            assert!(!factory_called.get());
            assert_eq!(captured.capabilities(), caps);
            assert_eq!(captured.budget(), budget);
            assert!(!parent.is_cancel_requested());
            assert_eq!(
                Cx::current().expect("parent restored").capabilities(),
                parent.capabilities()
            );
        });
    }

    #[test]
    fn migration_owner_drains_close_after_cancellation_and_pending() {
        use std::sync::Arc;
        use std::sync::atomic::{AtomicUsize, Ordering};

        let mut directory = tempfile::tempdir().expect("retained migration fixture");
        directory.disable_cleanup(true);
        let path = directory.path().join("cancelled-migration.sqlite");
        let runtime = build_asupersync_runtime().expect("native runtime");
        let closes = Arc::new(AtomicUsize::new(0));
        let cleanup_polls = std::cell::Cell::new(0);
        let result = runtime.block_on(async {
            let owner = Cx::current().expect("native owner");
            run_cli_operation(&owner, true, || async {
                let target = fsqlite::Connection::open(path.to_string_lossy().as_ref())
                    .await
                    .expect("open owned target");
                let observed_closes = Arc::clone(&closes);
                target.trace_v2(
                    fsqlite::TraceMask::CLOSE,
                    Some(Arc::new(move |event| {
                        if matches!(event, fsqlite::TraceEvent::Close) {
                            observed_closes.fetch_add(1, Ordering::SeqCst);
                        }
                    })),
                );
                target
                    .execute("CREATE TABLE demo (id INTEGER);")
                    .await
                    .unwrap();
                target.execute("BEGIN;").await.unwrap();
                target
                    .execute("INSERT INTO demo VALUES (42);")
                    .await
                    .unwrap();
                assert_eq!(target.query("SELECT id FROM demo;").await.unwrap().len(), 1);
                owner.cancel_with(asupersync::CancelKind::User, Some("migration cancellation"));
                for _ in 0..2 {
                    asupersync::runtime::yield_now().await;
                    cleanup_polls.set(cleanup_polls.get() + 1);
                }
                target
                    .close()
                    .await
                    .expect("await cancelled target teardown");
            })
            .await
        });
        assert_eq!(result, None, "cancelled migration must not report success");
        assert_eq!(cleanup_polls.get(), 2, "owner drove cleanup across Pending");
        assert_eq!(closes.load(Ordering::SeqCst), 1);

        runtime.block_on(async {
            let target = fsqlite::Connection::open(path.to_string_lossy().as_ref())
                .await
                .expect("reopen after awaited close");
            assert!(
                target
                    .query("SELECT id FROM demo;")
                    .await
                    .unwrap()
                    .is_empty()
            );
            target.execute("BEGIN;").await.unwrap();
            target
                .execute("INSERT INTO demo VALUES (7);")
                .await
                .unwrap();
            target.execute("COMMIT;").await.unwrap();
            target.close().await.unwrap();

            let owner = Cx::current().expect("native owner");
            owner.cancel_with(asupersync::CancelKind::User, Some("before migration"));
            let called = std::cell::Cell::new(false);
            let result = run_cli_operation(&owner, true, || {
                called.set(true);
                async { 1_u8 }
            })
            .await;
            assert_eq!(result, None);
            assert!(
                !called.get(),
                "pre-cancelled migration must not create resources"
            );
        });
    }

    #[test]
    fn build_version_includes_package_sha_and_timestamp() {
        let version = build_version();
        assert!(version.contains(env!("CARGO_PKG_VERSION")));
        assert!(version.contains(env!("VERGEN_GIT_SHA")));
        assert!(version.contains("built"));
    }
}
