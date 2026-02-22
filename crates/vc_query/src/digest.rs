//! Digest report generation
//!
//! Aggregates fleet health, alerts, usage, and notable events
//! into a concise daily/weekly summary.

use serde::{Deserialize, Serialize};
use std::fmt::Write as _;
use vc_store::VcStore;

// ============================================================================
// Digest sections
// ============================================================================

/// A digest section with title and items
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DigestSection {
    pub title: String,
    pub items: Vec<String>,
}

/// Complete digest report
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DigestReport {
    pub report_id: String,
    pub window_hours: u32,
    pub generated_at: String,
    pub sections: Vec<DigestSection>,
    pub summary: DigestSummary,
}

/// High-level summary numbers
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DigestSummary {
    pub total_machines: usize,
    pub machines_healthy: usize,
    pub machines_degraded: usize,
    pub open_alerts: usize,
    pub alerts_fired: usize,
    pub alerts_resolved: usize,
    pub collectors_healthy: usize,
    pub collectors_stale: usize,
}

// ============================================================================
// Report generator
// ============================================================================

/// Generate a digest report from the store
#[must_use]
pub fn generate_digest(store: &VcStore, window_hours: u32) -> DigestReport {
    let now = chrono::Utc::now();
    let report_id = format!("digest-{}h-{}", window_hours, now.timestamp());

    let mut sections = Vec::new();
    let mut summary = DigestSummary::default();

    // Section 1: Fleet overview
    let fleet_section = build_fleet_section(store, &mut summary);
    sections.push(fleet_section);

    // Section 2: Alert summary
    let alert_section = build_alert_section(store, &mut summary);
    sections.push(alert_section);

    // Section 3: Collector health
    let collector_section = build_collector_section(store, &mut summary);
    sections.push(collector_section);

    // Section 4: Notable events
    let events_section = build_events_section(store, window_hours);
    sections.push(events_section);

    DigestReport {
        report_id,
        window_hours,
        generated_at: now.to_rfc3339(),
        sections,
        summary,
    }
}

fn build_fleet_section(store: &VcStore, summary: &mut DigestSummary) -> DigestSection {
    let mut items = Vec::new();

    // Count machines
    let machines: usize = store
        .query_scalar::<i64>("SELECT COUNT(DISTINCT machine_id) FROM machines")
        .ok()
        .and_then(|value| usize::try_from(value).ok())
        .unwrap_or(0);
    summary.total_machines = machines;
    items.push(format!("Total machines: {machines}"));

    // Health scores
    let healthy: usize = store
        .query_scalar::<i64>("SELECT COUNT(*) FROM health_summary WHERE overall_score >= 80")
        .ok()
        .and_then(|value| usize::try_from(value).ok())
        .unwrap_or(0);
    let degraded: usize = store
        .query_scalar::<i64>(
            "SELECT COUNT(*) FROM health_summary WHERE overall_score < 80 AND overall_score >= 50",
        )
        .ok()
        .and_then(|value| usize::try_from(value).ok())
        .unwrap_or(0);
    let critical: usize = store
        .query_scalar::<i64>("SELECT COUNT(*) FROM health_summary WHERE overall_score < 50")
        .ok()
        .and_then(|value| usize::try_from(value).ok())
        .unwrap_or(0);

    summary.machines_healthy = healthy;
    summary.machines_degraded = degraded + critical;

    items.push(format!(
        "Healthy: {healthy}, Degraded: {degraded}, Critical: {critical}"
    ));

    DigestSection {
        title: "Fleet Overview".to_string(),
        items,
    }
}

fn build_alert_section(store: &VcStore, summary: &mut DigestSummary) -> DigestSection {
    let mut items = Vec::new();

    let open: usize = store
        .query_scalar::<i64>("SELECT COUNT(*) FROM alert_history WHERE resolved_at IS NULL")
        .ok()
        .and_then(|value| usize::try_from(value).ok())
        .unwrap_or(0);
    summary.open_alerts = open;
    items.push(format!("Open alerts: {open}"));

    // Recent alerts by severity
    let by_severity = store
        .query_json(
            "SELECT severity, COUNT(*) as cnt FROM alert_history \
             GROUP BY severity ORDER BY cnt DESC",
        )
        .unwrap_or_default();

    for entry in &by_severity {
        if let (Some(sev), Some(cnt)) = (entry["severity"].as_str(), entry["cnt"].as_i64()) {
            items.push(format!("  {sev}: {cnt}"));
        }
    }

    DigestSection {
        title: "Alert Summary".to_string(),
        items,
    }
}

fn build_collector_section(store: &VcStore, summary: &mut DigestSummary) -> DigestSection {
    let mut items = Vec::new();

    let healthy: usize = store
        .query_scalar::<i64>("SELECT COUNT(*) FROM collector_health WHERE success = true AND (freshness_seconds IS NULL OR freshness_seconds <= 600)")
        .ok()
        .and_then(|value| usize::try_from(value).ok())
        .unwrap_or(0);
    let stale: usize = store
        .query_scalar::<i64>("SELECT COUNT(*) FROM collector_health WHERE freshness_seconds > 600")
        .ok()
        .and_then(|value| usize::try_from(value).ok())
        .unwrap_or(0);

    summary.collectors_healthy = healthy;
    summary.collectors_stale = stale;

    items.push(format!("Healthy collectors: {healthy}"));
    items.push(format!("Stale collectors: {stale}"));

    DigestSection {
        title: "Collector Health".to_string(),
        items,
    }
}

fn build_events_section(store: &VcStore, window_hours: u32) -> DigestSection {
    let mut items = Vec::new();

    // Recent audit events
    let events = store
        .query_json(&format!(
            "SELECT event_type, COUNT(*) as cnt FROM audit_events \
             WHERE created_at >= current_timestamp - INTERVAL '{window_hours} hours' \
             GROUP BY event_type ORDER BY cnt DESC LIMIT 5"
        ))
        .unwrap_or_default();

    if events.is_empty() {
        items.push("No notable events in this window".to_string());
    } else {
        for event in &events {
            if let (Some(etype), Some(cnt)) = (event["event_type"].as_str(), event["cnt"].as_i64())
            {
                items.push(format!("{etype}: {cnt} events"));
            }
        }
    }

    DigestSection {
        title: "Notable Events".to_string(),
        items,
    }
}

// ============================================================================
// Markdown rendering
// ============================================================================

/// Render a digest report as Markdown
#[must_use]
pub fn render_markdown(report: &DigestReport) -> String {
    let mut md = String::new();

    let _ = write!(
        md,
        "# Vibe Cockpit Digest ({}h window)\n\n",
        report.window_hours
    );
    let _ = write!(md, "Generated: {}\n\n", report.generated_at);

    // Summary box
    md.push_str("## Summary\n\n");
    md.push_str("| Metric | Value |\n");
    md.push_str("| --- | --- |\n");
    let _ = writeln!(
        md,
        "| Machines | {} total, {} healthy |",
        report.summary.total_machines, report.summary.machines_healthy
    );
    let _ = writeln!(md, "| Alerts | {} open |", report.summary.open_alerts);
    let _ = writeln!(
        md,
        "| Collectors | {} healthy, {} stale |",
        report.summary.collectors_healthy, report.summary.collectors_stale
    );
    md.push('\n');

    // Sections
    for section in &report.sections {
        let _ = write!(md, "## {}\n\n", section.title);
        for item in &section.items {
            let _ = writeln!(md, "- {item}");
        }
        md.push('\n');
    }

    md
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    fn test_store() -> VcStore {
        VcStore::open_memory().unwrap()
    }

    // ========================================================================
    // DigestReport tests
    // ========================================================================

    #[test]
    fn test_generate_digest_empty_db() {
        let store = test_store();
        let report = generate_digest(&store, 24);
        assert_eq!(report.window_hours, 24);
        assert!(!report.report_id.is_empty());
        assert!(report.sections.len() >= 4);
    }

    #[test]
    fn test_generate_digest_weekly() {
        let store = test_store();
        let report = generate_digest(&store, 168);
        assert_eq!(report.window_hours, 168);
        assert!(report.report_id.contains("168h"));
    }

    #[test]
    fn test_digest_summary_defaults() {
        let summary = DigestSummary::default();
        assert_eq!(summary.total_machines, 0);
        assert_eq!(summary.open_alerts, 0);
    }

    #[test]
    fn test_digest_report_serialization() {
        let store = test_store();
        let report = generate_digest(&store, 24);
        let json = serde_json::to_string(&report).unwrap();
        let parsed: DigestReport = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.window_hours, 24);
    }

    // ========================================================================
    // Section tests
    // ========================================================================

    #[test]
    fn test_fleet_section() {
        let store = test_store();
        let mut summary = DigestSummary::default();
        let section = build_fleet_section(&store, &mut summary);
        assert_eq!(section.title, "Fleet Overview");
        assert!(!section.items.is_empty());
    }

    #[test]
    fn test_alert_section() {
        let store = test_store();
        let mut summary = DigestSummary::default();
        let section = build_alert_section(&store, &mut summary);
        assert_eq!(section.title, "Alert Summary");
    }

    #[test]
    fn test_collector_section() {
        let store = test_store();
        let mut summary = DigestSummary::default();
        let section = build_collector_section(&store, &mut summary);
        assert_eq!(section.title, "Collector Health");
    }

    #[test]
    fn test_events_section() {
        let store = test_store();
        let section = build_events_section(&store, 24);
        assert_eq!(section.title, "Notable Events");
        assert!(!section.items.is_empty());
    }

    // ========================================================================
    // Markdown rendering tests
    // ========================================================================

    #[test]
    fn test_render_markdown() {
        let store = test_store();
        let report = generate_digest(&store, 24);
        let md = render_markdown(&report);
        assert!(md.contains("# Vibe Cockpit Digest"));
        assert!(md.contains("24h window"));
        assert!(md.contains("## Summary"));
        assert!(md.contains("## Fleet Overview"));
        assert!(md.contains("## Alert Summary"));
    }

    #[test]
    fn test_render_markdown_has_table() {
        let store = test_store();
        let report = generate_digest(&store, 24);
        let md = render_markdown(&report);
        assert!(md.contains("| Metric | Value |"));
        assert!(md.contains("| Machines |"));
    }

    #[test]
    fn test_render_markdown_weekly() {
        let store = test_store();
        let report = generate_digest(&store, 168);
        let md = render_markdown(&report);
        assert!(md.contains("168h window"));
    }

    // ========================================================================
    // Store integration tests
    // ========================================================================

    #[test]
    fn test_store_digest_report() {
        let store = test_store();
        let report = generate_digest(&store, 24);
        let json = serde_json::to_string(&report.summary).unwrap();
        let md = render_markdown(&report);

        store
            .insert_digest_report(&report.report_id, 24, &json, &md)
            .unwrap();

        let retrieved = store.get_digest_report(&report.report_id).unwrap();
        assert!(retrieved.is_some());
    }

    #[test]
    fn test_list_digest_reports() {
        let store = test_store();

        // Generate two reports
        let r1 = generate_digest(&store, 24);
        let r2 = generate_digest(&store, 168);

        store
            .insert_digest_report(&r1.report_id, 24, "{}", "# daily")
            .unwrap();
        store
            .insert_digest_report(&r2.report_id, 168, "{}", "# weekly")
            .unwrap();

        let reports = store.list_digest_reports(10).unwrap();
        assert_eq!(reports.len(), 2);
    }

    // ========================================================================
    // DigestSection tests
    // ========================================================================

    #[test]
    fn test_digest_section_serialization() {
        let section = DigestSection {
            title: "Test".to_string(),
            items: vec!["item1".to_string(), "item2".to_string()],
        };
        let json = serde_json::to_string(&section).unwrap();
        let parsed: DigestSection = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.items.len(), 2);
    }
}
