//! Health score computation from live telemetry.
//!
//! This module is the missing link between the collectors (which write raw
//! telemetry into `sys_samples`, `sys_fallback_samples`, `sys_filesystems`,
//! `account_usage_snapshots` and `collector_health`) and the health tables
//! (`health_summary` / `health_factors`) that `fleet_overview`,
//! `machine_health` and the TUI read from.
//!
//! The daemon tick calls [`QueryBuilder::compute_and_persist_health_all`],
//! which reads current telemetry for every enabled machine, classifies each
//! metric with [`crate::classify_metric`], weights it with
//! [`crate::HealthWeights`] and persists the result through
//! [`QueryBuilder::persist_health_score`].
//!
//! ## `DuckDB` timestamp handling
//!
//! Every `collected_at` column in the schema is declared `TEXT`. Comparing
//! such a column against `current_timestamp` (a `TIMESTAMPTZ`) is a Binder
//! Error in `DuckDB`, so **no** SQL in this module does time arithmetic: the
//! SQL only ever compares `collected_at` against `collected_at` (same type),
//! and all age/window math is done in Rust after parsing the text timestamp.

use chrono::{DateTime, NaiveDateTime, TimeZone, Utc};

use crate::{HealthFactor, HealthScore, HealthWeights, QueryBuilder, QueryError, classify_metric};

/// CPU utilisation percentage that counts as a warning.
const CPU_WARNING_PCT: f64 = 75.0;
/// CPU utilisation percentage that counts as critical.
const CPU_CRITICAL_PCT: f64 = 90.0;

/// Memory utilisation percentage that counts as a warning.
const MEM_WARNING_PCT: f64 = 80.0;
/// Memory utilisation percentage that counts as critical.
const MEM_CRITICAL_PCT: f64 = 92.0;

/// Load average per core that counts as a warning.
const LOAD_WARNING_PER_CORE: f64 = 1.0;
/// Load average per core that counts as critical.
const LOAD_CRITICAL_PER_CORE: f64 = 2.0;

/// Filesystem utilisation percentage that counts as a warning.
const DISK_WARNING_PCT: f64 = 85.0;
/// Filesystem utilisation percentage that counts as critical.
const DISK_CRITICAL_PCT: f64 = 95.0;

/// Provider quota consumption percentage that counts as a warning.
const RATE_LIMIT_WARNING_PCT: f64 = 80.0;
/// Provider quota consumption percentage that counts as critical.
const RATE_LIMIT_CRITICAL_PCT: f64 = 95.0;

/// Age (seconds) of the newest successful collector run that counts as a warning.
const FRESHNESS_WARNING_SECS: f64 = 600.0;
/// Age (seconds) of the newest successful collector run that counts as critical.
const FRESHNESS_CRITICAL_SECS: f64 = 1800.0;
/// Age assigned when a machine has never had a successful collector run.
const FRESHNESS_NEVER_SECS: f64 = 86_400.0;

/// Collector success rate (percent) below which we warn.
const COLLECTOR_SUCCESS_WARNING_PCT: f64 = 95.0;
/// Collector success rate (percent) below which we go critical.
const COLLECTOR_SUCCESS_CRITICAL_PCT: f64 = 60.0;

/// Window (seconds) over which the collector success rate is computed.
const COLLECTOR_WINDOW_SECS: i64 = 3600;
/// Cap on how many `collector_health` rows we pull per machine.
const COLLECTOR_ROW_LIMIT: usize = 500;

/// Parse a timestamp that the collectors wrote into a `TEXT` column.
///
/// Collectors write RFC3339, but `DuckDB` may hand back a plain
/// `YYYY-MM-DD HH:MM:SS[.ffffff]` rendering, so both are accepted.
fn parse_stored_timestamp(raw: &str) -> Option<DateTime<Utc>> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }
    if let Ok(dt) = DateTime::parse_from_rfc3339(trimmed) {
        return Some(dt.with_timezone(&Utc));
    }
    for fmt in [
        "%Y-%m-%d %H:%M:%S%.f",
        "%Y-%m-%dT%H:%M:%S%.f",
        "%Y-%m-%d %H:%M:%S",
        "%Y-%m-%dT%H:%M:%S",
    ] {
        if let Ok(naive) = NaiveDateTime::parse_from_str(trimmed, fmt) {
            return Some(Utc.from_utc_datetime(&naive));
        }
    }
    None
}

/// One metric to be classified into a health factor.
struct FactorSpec<'s> {
    /// Factor id; must match a [`HealthWeights`] key to get a non-default weight.
    factor_id: &'s str,
    /// Human-readable factor name.
    name: &'s str,
    /// Observed metric value.
    value: f64,
    /// Threshold at which the metric is a warning.
    warning: f64,
    /// Threshold at which the metric is critical.
    critical: f64,
    /// `true` when a *lower* value is worse (e.g. a success rate).
    inverted: bool,
    /// Human-readable explanation of the observed value.
    details: String,
}

/// Build a single weighted, classified health factor.
fn build_factor(weights: &HealthWeights, spec: FactorSpec<'_>) -> HealthFactor {
    let (score, severity) = classify_metric(spec.value, spec.warning, spec.critical, spec.inverted);
    HealthFactor {
        factor_id: spec.factor_id.to_string(),
        name: spec.name.to_string(),
        score,
        weight: weights.weight_for(spec.factor_id),
        severity,
        details: spec.details,
    }
}

/// Latest system sample for a machine, from `sys_samples` with a fall back to
/// the always-on `sys_fallback_samples` baseline probe.
#[derive(Debug, Default, Clone)]
struct SysSample {
    cpu_pct: Option<f64>,
    load1: Option<f64>,
    core_count: Option<f64>,
    mem_pct: Option<f64>,
    /// Worst filesystem usage percent seen in the fallback probe payload.
    fallback_disk_pct: Option<f64>,
    source: &'static str,
}

/// Compute memory usage percent from raw byte counters.
fn memory_pct(row: &serde_json::Value) -> Option<f64> {
    let total = row["mem_total_bytes"].as_f64()?;
    if total <= 0.0 {
        return None;
    }
    let used = row["mem_used_bytes"].as_f64().or_else(|| {
        let available = row["mem_available_bytes"].as_f64()?;
        Some(total - available)
    })?;
    Some((used / total * 100.0).clamp(0.0, 100.0))
}

/// Extract the worst `pct` from a `sys_fallback_samples.disk_usage_json` payload.
///
/// The payload is `[{mount, total, used, avail, pct}, ...]`.
fn worst_disk_pct_from_payload(raw: &str) -> Option<f64> {
    let parsed: serde_json::Value = serde_json::from_str(raw).ok()?;
    let entries = parsed.as_array()?;
    entries
        .iter()
        .filter_map(|entry| entry["pct"].as_f64())
        .fold(None, |acc: Option<f64>, pct| {
            Some(acc.map_or(pct, |best| best.max(pct)))
        })
}

impl QueryBuilder<'_> {
    /// Compute health factors for a machine from its current telemetry.
    ///
    /// Emits, when the underlying telemetry exists: `sys_cpu`, `sys_memory`,
    /// `sys_load`, `sys_disk`, `rate_limit`, `process_health`. `data_freshness`
    /// is always emitted so that a machine with no telemetry at all scores
    /// badly instead of silently scoring "perfectly healthy".
    ///
    /// # Errors
    ///
    /// Returns [`QueryError`] if any underlying store query fails.
    pub fn compute_health_factors(
        &self,
        machine_id: &str,
    ) -> Result<Vec<HealthFactor>, QueryError> {
        let weights = HealthWeights::default();
        let mut factors = Vec::new();

        let sample = self.latest_sys_sample(machine_id)?;

        if let Some(cpu_pct) = sample.cpu_pct {
            factors.push(build_factor(
                &weights,
                FactorSpec {
                    factor_id: "sys_cpu",
                    name: "CPU utilization",
                    value: cpu_pct,
                    warning: CPU_WARNING_PCT,
                    critical: CPU_CRITICAL_PCT,
                    inverted: false,
                    details: format!("cpu {cpu_pct:.1}% (source: {})", sample.source),
                },
            ));
        }

        if let Some(mem_pct) = sample.mem_pct {
            factors.push(build_factor(
                &weights,
                FactorSpec {
                    factor_id: "sys_memory",
                    name: "Memory utilization",
                    value: mem_pct,
                    warning: MEM_WARNING_PCT,
                    critical: MEM_CRITICAL_PCT,
                    inverted: false,
                    details: format!("memory {mem_pct:.1}% used (source: {})", sample.source),
                },
            ));
        }

        if let Some(load1) = sample.load1 {
            let cores = sample.core_count.filter(|c| *c >= 1.0).unwrap_or(1.0);
            let per_core = load1 / cores;
            factors.push(build_factor(
                &weights,
                FactorSpec {
                    factor_id: "sys_load",
                    name: "Load average",
                    value: per_core,
                    warning: LOAD_WARNING_PER_CORE,
                    critical: LOAD_CRITICAL_PER_CORE,
                    inverted: false,
                    details: format!(
                        "load1 {load1:.2} over {cores:.0} core(s) = {per_core:.2}/core"
                    ),
                },
            ));
        }

        // Prefer the detailed sysmoni snapshot, fall back to the baseline probe.
        let disk_pct = self
            .worst_filesystem_pct(machine_id)?
            .or(sample.fallback_disk_pct);
        if let Some(disk_pct) = disk_pct {
            factors.push(build_factor(
                &weights,
                FactorSpec {
                    factor_id: "sys_disk",
                    name: "Disk utilization",
                    value: disk_pct,
                    warning: DISK_WARNING_PCT,
                    critical: DISK_CRITICAL_PCT,
                    inverted: false,
                    details: format!("worst filesystem {disk_pct:.1}% full"),
                },
            ));
        }

        if let Some(usage_pct) = self.worst_account_usage_pct(machine_id)? {
            factors.push(build_factor(
                &weights,
                FactorSpec {
                    factor_id: "rate_limit",
                    name: "Provider quota",
                    value: usage_pct,
                    warning: RATE_LIMIT_WARNING_PCT,
                    critical: RATE_LIMIT_CRITICAL_PCT,
                    inverted: false,
                    details: format!("worst account quota {usage_pct:.1}% consumed"),
                },
            ));
        }

        let collectors = self.collector_health_stats(machine_id)?;

        let (age_secs, freshness_detail) = match collectors.newest_success_age_secs {
            Some(age) => (age, format!("last successful collection {age:.0}s ago")),
            None => (
                FRESHNESS_NEVER_SECS,
                "no successful collector run on record".to_string(),
            ),
        };
        factors.push(build_factor(
            &weights,
            FactorSpec {
                factor_id: "data_freshness",
                name: "Data freshness",
                value: age_secs,
                warning: FRESHNESS_WARNING_SECS,
                critical: FRESHNESS_CRITICAL_SECS,
                inverted: false,
                details: freshness_detail,
            },
        ));

        if let Some(success_pct) = collectors.success_pct_in_window {
            factors.push(build_factor(
                &weights,
                FactorSpec {
                    factor_id: "process_health",
                    name: "Collector success rate",
                    value: success_pct,
                    warning: COLLECTOR_SUCCESS_WARNING_PCT,
                    critical: COLLECTOR_SUCCESS_CRITICAL_PCT,
                    // Lower success rate is worse.
                    inverted: true,
                    details: format!(
                        "{}/{} collector runs succeeded in the last hour",
                        collectors.successes_in_window, collectors.runs_in_window
                    ),
                },
            ));
        }

        Ok(factors)
    }

    /// Compute the current health of a machine from telemetry and persist it
    /// into `health_summary` + `health_factors`.
    ///
    /// # Errors
    ///
    /// Returns [`QueryError`] if telemetry reads or the health writes fail.
    pub fn compute_and_persist_health(&self, machine_id: &str) -> Result<HealthScore, QueryError> {
        let factors = self.compute_health_factors(machine_id)?;
        self.persist_health_score(machine_id, &factors)
    }

    /// Compute and persist health for every enabled machine in the registry.
    ///
    /// This is the entry point for the daemon tick. Machines are processed in a
    /// stable order and one failing machine aborts the tick (the store is the
    /// same for all of them, so a failure is a store-level failure).
    ///
    /// # Errors
    ///
    /// Returns [`QueryError`] if telemetry reads or the health writes fail.
    pub fn compute_and_persist_health_all(&self) -> Result<Vec<HealthScore>, QueryError> {
        let sql = "SELECT machine_id FROM machines \
                   WHERE enabled IS NULL OR enabled <> 0 \
                   ORDER BY machine_id";
        let rows = self.store.query_json(sql)?;

        let mut scores = Vec::with_capacity(rows.len());
        for row in &rows {
            let Some(machine_id) = row["machine_id"].as_str() else {
                continue;
            };
            scores.push(self.compute_and_persist_health(machine_id)?);
        }
        Ok(scores)
    }

    /// Latest system sample, preferring `sys_samples` and falling back to the
    /// always-on `fallback_probe` baseline.
    fn latest_sys_sample(&self, machine_id: &str) -> Result<SysSample, QueryError> {
        let escaped = vc_store::escape_sql_literal(machine_id);

        let sql = format!(
            "SELECT cpu_total, load1, core_count, mem_used_bytes, mem_total_bytes, \
             mem_available_bytes \
             FROM sys_samples WHERE machine_id = '{escaped}' \
             ORDER BY collected_at DESC LIMIT 1"
        );
        let rows = self.store.query_json(&sql)?;
        if let Some(row) = rows.first() {
            return Ok(SysSample {
                cpu_pct: row["cpu_total"].as_f64(),
                load1: row["load1"].as_f64(),
                core_count: row["core_count"].as_f64(),
                mem_pct: memory_pct(row),
                fallback_disk_pct: None,
                source: "sysmoni",
            });
        }

        let sql = format!(
            "SELECT load1, mem_used_bytes, mem_total_bytes, mem_available_bytes, \
             disk_usage_json \
             FROM sys_fallback_samples WHERE machine_id = '{escaped}' \
             ORDER BY collected_at DESC LIMIT 1"
        );
        let rows = self.store.query_json(&sql)?;
        let Some(row) = rows.first() else {
            return Ok(SysSample::default());
        };

        Ok(SysSample {
            // The fallback probe has no CPU percentage; load average stands in.
            cpu_pct: None,
            load1: row["load1"].as_f64(),
            core_count: None,
            mem_pct: memory_pct(row),
            fallback_disk_pct: row["disk_usage_json"]
                .as_str()
                .and_then(worst_disk_pct_from_payload),
            source: "fallback_probe",
        })
    }

    /// Worst filesystem usage percent in the most recent `sys_filesystems` snapshot.
    fn worst_filesystem_pct(&self, machine_id: &str) -> Result<Option<f64>, QueryError> {
        let escaped = vc_store::escape_sql_literal(machine_id);
        // NOTE: `collected_at` is TEXT; it is only ever compared against itself
        // here, never against `current_timestamp`, so DuckDB has no type clash.
        let sql = format!(
            "SELECT MAX(usage_pct) AS worst_pct FROM sys_filesystems \
             WHERE machine_id = '{escaped}' AND collected_at = ( \
                 SELECT MAX(collected_at) FROM sys_filesystems \
                 WHERE machine_id = '{escaped}' \
             )"
        );
        let rows = self.store.query_json(&sql)?;
        Ok(rows.first().and_then(|row| row["worst_pct"].as_f64()))
    }

    /// Worst provider quota consumption in the most recent usage snapshot.
    fn worst_account_usage_pct(&self, machine_id: &str) -> Result<Option<f64>, QueryError> {
        let escaped = vc_store::escape_sql_literal(machine_id);
        let sql = format!(
            "SELECT MAX(usage_pct) AS worst_pct FROM account_usage_snapshots \
             WHERE machine_id = '{escaped}' AND collected_at = ( \
                 SELECT MAX(collected_at) FROM account_usage_snapshots \
                 WHERE machine_id = '{escaped}' \
             )"
        );
        let rows = self.store.query_json(&sql)?;
        Ok(rows.first().and_then(|row| row["worst_pct"].as_f64()))
    }

    /// Freshness + success-rate statistics derived from `collector_health`.
    fn collector_health_stats(&self, machine_id: &str) -> Result<CollectorStats, QueryError> {
        let escaped = vc_store::escape_sql_literal(machine_id);
        // `collected_at` is TEXT, so the window filter is applied in Rust after
        // parsing rather than with a `current_timestamp` comparison in SQL.
        let sql = format!(
            "SELECT success, CAST(collected_at AS TEXT) AS collected_at \
             FROM collector_health WHERE machine_id = '{escaped}' \
             ORDER BY collected_at DESC LIMIT {COLLECTOR_ROW_LIMIT}"
        );
        let rows = self.store.query_json(&sql)?;

        let now = Utc::now();
        let mut stats = CollectorStats::default();

        for row in &rows {
            let Some(ts) = row["collected_at"]
                .as_str()
                .and_then(parse_stored_timestamp)
            else {
                continue;
            };
            let success = match &row["success"] {
                serde_json::Value::Bool(b) => *b,
                other => other.as_i64().unwrap_or(0) != 0,
            };

            let age_secs = (now - ts).num_seconds();

            if success {
                let age_f64 = f64::from(i32::try_from(age_secs.max(0)).unwrap_or(i32::MAX));
                stats.newest_success_age_secs = Some(
                    stats
                        .newest_success_age_secs
                        .map_or(age_f64, |best| best.min(age_f64)),
                );
            }

            if age_secs <= COLLECTOR_WINDOW_SECS {
                stats.runs_in_window += 1;
                if success {
                    stats.successes_in_window += 1;
                }
            }
        }

        if stats.runs_in_window > 0 {
            let runs = f64::from(u32::try_from(stats.runs_in_window).unwrap_or(u32::MAX));
            let successes = f64::from(u32::try_from(stats.successes_in_window).unwrap_or(u32::MAX));
            stats.success_pct_in_window = Some(successes / runs * 100.0);
        }

        Ok(stats)
    }
}

/// Aggregated `collector_health` statistics for one machine.
#[derive(Debug, Default, Clone)]
struct CollectorStats {
    /// Age in seconds of the newest successful collector run, if any.
    newest_success_age_secs: Option<f64>,
    /// Collector runs recorded within [`COLLECTOR_WINDOW_SECS`].
    runs_in_window: usize,
    /// Successful runs within [`COLLECTOR_WINDOW_SECS`].
    successes_in_window: usize,
    /// Success rate over the window, `None` when the window is empty.
    success_pct_in_window: Option<f64>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Severity;
    use std::fmt::Write;
    use vc_store::VcStore;

    /// An RFC3339 timestamp `secs_ago` seconds in the past.
    fn ts_ago(secs_ago: i64) -> String {
        (Utc::now() - chrono::Duration::seconds(secs_ago)).to_rfc3339()
    }

    fn store_with_machine(machine_id: &str) -> VcStore {
        let store = VcStore::open_memory().unwrap();
        store
            .execute_batch(&format!(
                "INSERT INTO machines (machine_id, hostname, status, enabled) \
                 VALUES ('{machine_id}', '{machine_id}-host', 'online', 1);"
            ))
            .unwrap();
        store
    }

    fn factor<'f>(factors: &'f [HealthFactor], id: &str) -> Option<&'f HealthFactor> {
        factors.iter().find(|f| f.factor_id == id)
    }

    #[test]
    fn test_parse_stored_timestamp_formats() {
        assert!(parse_stored_timestamp("2026-07-11T12:00:00Z").is_some());
        assert!(parse_stored_timestamp("2026-07-11T12:00:00+00:00").is_some());
        assert!(parse_stored_timestamp("2026-07-11 12:00:00").is_some());
        assert!(parse_stored_timestamp("2026-07-11 12:00:00.123456").is_some());
        assert!(parse_stored_timestamp("").is_none());
        assert!(parse_stored_timestamp("not-a-timestamp").is_none());
    }

    #[test]
    fn test_worst_disk_pct_from_payload() {
        let payload = r#"[{"mount":"/","pct":41.5},{"mount":"/data","pct":93.2}]"#;
        let worst = worst_disk_pct_from_payload(payload).unwrap();
        assert!((worst - 93.2).abs() < 1e-6);

        assert!(worst_disk_pct_from_payload("[]").is_none());
        assert!(worst_disk_pct_from_payload("garbage").is_none());
    }

    #[test]
    fn test_memory_pct_from_used_bytes() {
        let row = serde_json::json!({"mem_used_bytes": 8_000_000_000_u64, "mem_total_bytes": 16_000_000_000_u64});
        let pct = memory_pct(&row).unwrap();
        assert!((pct - 50.0).abs() < 1e-6);
    }

    #[test]
    fn test_memory_pct_from_available_bytes() {
        let row = serde_json::json!({"mem_available_bytes": 4_000_000_000_u64, "mem_total_bytes": 16_000_000_000_u64});
        let pct = memory_pct(&row).unwrap();
        assert!((pct - 75.0).abs() < 1e-6);
    }

    #[test]
    fn test_memory_pct_zero_total_is_none() {
        let row = serde_json::json!({"mem_used_bytes": 1, "mem_total_bytes": 0});
        assert!(memory_pct(&row).is_none());
    }

    #[test]
    fn test_no_telemetry_yields_critical_freshness() {
        let store = store_with_machine("m1");
        let qb = QueryBuilder::new(&store);

        let factors = qb.compute_health_factors("m1").unwrap();
        // Only data_freshness, and it must be critical: a machine we have never
        // heard from is not "healthy".
        assert_eq!(factors.len(), 1);
        let freshness = factor(&factors, "data_freshness").unwrap();
        assert_eq!(freshness.severity, Severity::Critical);
        assert!(freshness.score < f64::EPSILON);
    }

    #[test]
    fn test_healthy_machine_scores_high() {
        let store = store_with_machine("m1");
        let now = ts_ago(30);
        store
            .execute_batch(&format!(
                "INSERT INTO sys_samples \
                   (machine_id, collected_at, cpu_total, load1, core_count, \
                    mem_used_bytes, mem_total_bytes) \
                 VALUES ('m1', '{now}', 12.5, 1.0, 8, 4000000000, 16000000000); \
                 INSERT INTO sys_filesystems \
                   (machine_id, collected_at, mount, total_bytes, used_bytes, usage_pct) \
                 VALUES ('m1', '{now}', '/', 1000, 300, 30.0); \
                 INSERT INTO account_usage_snapshots \
                   (machine_id, collected_at, provider, account_id, usage_pct) \
                 VALUES ('m1', '{now}', 'anthropic', 'a1', 20.0); \
                 INSERT INTO collector_health \
                   (machine_id, collector, collected_at, success) \
                 VALUES ('m1', 'sysmoni', '{now}', 1);"
            ))
            .unwrap();

        let qb = QueryBuilder::new(&store);
        let factors = qb.compute_health_factors("m1").unwrap();

        // cpu, memory, load, disk, rate_limit, freshness, process_health
        assert_eq!(factors.len(), 7);
        assert!(factors.iter().all(|f| f.severity == Severity::Healthy));
        assert!(factor(&factors, "sys_cpu").is_some());
        assert!(factor(&factors, "process_health").is_some());

        let score = crate::compute_overall_score(&factors);
        assert!((score - 1.0).abs() < f64::EPSILON, "score was {score}");
    }

    #[test]
    fn test_degraded_machine_produces_critical_factors() {
        let store = store_with_machine("m1");
        let now = ts_ago(10);
        store
            .execute_batch(&format!(
                "INSERT INTO sys_samples \
                   (machine_id, collected_at, cpu_total, load1, core_count, \
                    mem_used_bytes, mem_total_bytes) \
                 VALUES ('m1', '{now}', 97.0, 16.0, 4, 15500000000, 16000000000); \
                 INSERT INTO sys_filesystems \
                   (machine_id, collected_at, mount, total_bytes, used_bytes, usage_pct) \
                 VALUES ('m1', '{now}', '/', 1000, 990, 99.0); \
                 INSERT INTO account_usage_snapshots \
                   (machine_id, collected_at, provider, account_id, usage_pct) \
                 VALUES ('m1', '{now}', 'anthropic', 'a1', 99.5); \
                 INSERT INTO collector_health \
                   (machine_id, collector, collected_at, success) \
                 VALUES ('m1', 'sysmoni', '{now}', 1);"
            ))
            .unwrap();

        let qb = QueryBuilder::new(&store);
        let factors = qb.compute_health_factors("m1").unwrap();

        assert_eq!(
            factor(&factors, "sys_cpu").unwrap().severity,
            Severity::Critical
        );
        assert_eq!(
            factor(&factors, "sys_memory").unwrap().severity,
            Severity::Critical
        );
        assert_eq!(
            factor(&factors, "sys_disk").unwrap().severity,
            Severity::Critical
        );
        assert_eq!(
            factor(&factors, "sys_load").unwrap().severity,
            Severity::Critical
        );
        assert_eq!(
            factor(&factors, "rate_limit").unwrap().severity,
            Severity::Critical
        );

        let score = crate::compute_overall_score(&factors);
        assert!(score < 0.3, "expected a bad score, got {score}");
    }

    #[test]
    fn test_fallback_probe_only_machine_is_scored() {
        let store = store_with_machine("m1");
        let now = ts_ago(20);
        store
            .execute_batch(&format!(
                "INSERT INTO sys_fallback_samples \
                   (machine_id, collected_at, load1, mem_total_bytes, mem_available_bytes, \
                    disk_usage_json) \
                 VALUES ('m1', '{now}', 0.5, 16000000000, 12000000000, \
                         '[{{\"mount\":\"/\",\"pct\":97.0}}]'); \
                 INSERT INTO collector_health \
                   (machine_id, collector, collected_at, success) \
                 VALUES ('m1', 'fallback_probe', '{now}', 1);"
            ))
            .unwrap();

        let qb = QueryBuilder::new(&store);
        let factors = qb.compute_health_factors("m1").unwrap();

        // No CPU sample from the fallback probe.
        assert!(factor(&factors, "sys_cpu").is_none());
        // Memory comes from total - available = 25%.
        let mem = factor(&factors, "sys_memory").unwrap();
        assert_eq!(mem.severity, Severity::Healthy);
        // Disk comes from the fallback probe's JSON payload.
        let disk = factor(&factors, "sys_disk").unwrap();
        assert_eq!(disk.severity, Severity::Critical);
    }

    #[test]
    fn test_stale_telemetry_flags_freshness() {
        let store = store_with_machine("m1");
        let stale = ts_ago(7200);
        store
            .execute_batch(&format!(
                "INSERT INTO collector_health \
                   (machine_id, collector, collected_at, success) \
                 VALUES ('m1', 'sysmoni', '{stale}', 1);"
            ))
            .unwrap();

        let qb = QueryBuilder::new(&store);
        let factors = qb.compute_health_factors("m1").unwrap();

        let freshness = factor(&factors, "data_freshness").unwrap();
        assert_eq!(freshness.severity, Severity::Critical);
        // Outside the one-hour window, so no success-rate factor.
        assert!(factor(&factors, "process_health").is_none());
    }

    #[test]
    fn test_collector_failures_degrade_process_health() {
        let store = store_with_machine("m1");
        let mut sql = String::new();
        for i in 0..10 {
            let ts = ts_ago(60 * (i + 1));
            let success = i32::from(i < 5);
            write!(
                &mut sql,
                "INSERT INTO collector_health \
                   (machine_id, collector, collected_at, success) \
                 VALUES ('m1', 'c{i}', '{ts}', {success}); "
            )
            .expect("writing to a String cannot fail");
        }
        store.execute_batch(&sql).unwrap();

        let qb = QueryBuilder::new(&store);
        let factors = qb.compute_health_factors("m1").unwrap();

        let process = factor(&factors, "process_health").unwrap();
        // 50% success rate is below the 60% critical floor.
        assert_eq!(process.severity, Severity::Critical);
        assert!(process.score < f64::EPSILON);
    }

    #[test]
    fn test_compute_and_persist_health_writes_tables() {
        let store = store_with_machine("m1");
        let now = ts_ago(5);
        store
            .execute_batch(&format!(
                "INSERT INTO sys_samples \
                   (machine_id, collected_at, cpu_total, load1, core_count, \
                    mem_used_bytes, mem_total_bytes) \
                 VALUES ('m1', '{now}', 80.0, 1.0, 4, 8000000000, 16000000000); \
                 INSERT INTO collector_health \
                   (machine_id, collector, collected_at, success) \
                 VALUES ('m1', 'sysmoni', '{now}', 1);"
            ))
            .unwrap();

        let qb = QueryBuilder::new(&store);
        let score = qb.compute_and_persist_health("m1").unwrap();
        assert_eq!(score.machine_id, "m1");
        assert!(!score.factors.is_empty());

        // health_summary and health_factors are no longer empty.
        let summaries = qb.list_health_summaries().unwrap();
        assert_eq!(summaries.len(), 1);
        assert_eq!(summaries[0]["machine_id"].as_str().unwrap(), "m1");

        // machine_health reads back what we just wrote.
        let health = qb.machine_health("m1").unwrap();
        assert!((health.overall_score - score.overall_score).abs() < 1e-9);
        assert_eq!(health.factors.len(), score.factors.len());
        assert!(health.worst_factor.is_some());

        let cpu = factor(&health.factors, "sys_cpu").unwrap();
        assert_eq!(cpu.severity, Severity::Warning);
    }

    #[test]
    fn test_compute_and_persist_health_all_covers_enabled_machines() {
        let store = VcStore::open_memory().unwrap();
        let now = ts_ago(5);
        store
            .execute_batch(&format!(
                "INSERT INTO machines (machine_id, hostname, status, enabled) \
                 VALUES ('m1', 'alpha', 'online', 1); \
                 INSERT INTO machines (machine_id, hostname, status, enabled) \
                 VALUES ('m2', 'bravo', 'online', 1); \
                 INSERT INTO machines (machine_id, hostname, status, enabled) \
                 VALUES ('m3', 'charlie', 'offline', 0); \
                 INSERT INTO sys_samples \
                   (machine_id, collected_at, cpu_total, load1, core_count, \
                    mem_used_bytes, mem_total_bytes) \
                 VALUES ('m1', '{now}', 5.0, 0.1, 8, 1000000000, 16000000000); \
                 INSERT INTO collector_health \
                   (machine_id, collector, collected_at, success) \
                 VALUES ('m1', 'sysmoni', '{now}', 1);"
            ))
            .unwrap();

        let qb = QueryBuilder::new(&store);
        let scores = qb.compute_and_persist_health_all().unwrap();

        // m3 is disabled and must be skipped.
        assert_eq!(scores.len(), 2);
        assert_eq!(scores[0].machine_id, "m1");
        assert_eq!(scores[1].machine_id, "m2");

        // m1 has fresh, healthy telemetry; m2 has none at all.
        assert!(scores[0].overall_score > 0.9);
        assert!(scores[1].overall_score < 0.1);

        // fleet_overview now has real data to work with.
        let overview = qb.fleet_overview().unwrap();
        assert!(overview.fleet_health_score < 1.0);
        assert_eq!(overview.worst_machine, Some("m2".to_string()));
    }
}
