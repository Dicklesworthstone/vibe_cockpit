//! TOON (Token-Optimized Object Notation) output format
//!
//! TOON reduces JSON output by 60-70% for agent consumption while preserving
//! essential information for decision-making.
//!
//! Format specification:
//! - Version header: `TOON1|`
//! - Sections delimited by `|`
//! - Items within sections delimited by `,`
//! - Key-value pairs use `:`
//! - `!` prefix indicates alert/warning severity
//! - `@` indicates location/time relationship
//! - `%` suffix indicates percentage
//!
//! Section codes:
//! - `F:` Fleet summary
//! - `M:` Machines
//! - `AL:` Alerts
//! - `PR:` Predictions
//! - `AC:` Accounts
//! - `RP:` Repos
//! - `EV:` Events
//! - `TR:` Triage recommendations
//! - `KB:` Knowledge base results

use crate::robot::{HealthData, MachineHealth, StatusData, TriageData};
use serde::Serialize;

/// Trait for types that can be serialized to TOON format
pub trait ToToon {
    fn to_toon(&self) -> String;
}

/// Convert a HealthData to TOON format
///
/// Example output:
/// ```text
/// TOON1|F:1on0off,h100,ag0,al0|M:local:on,h100|AL:0c0w0i
/// ```
impl ToToon for HealthData {
    fn to_toon(&self) -> String {
        let mut parts = vec!["TOON1".to_string()];

        // Fleet summary section
        let online = self
            .machines
            .iter()
            .filter(|m| m.status == "online")
            .count();
        let offline = self.machines.len() - online;
        let fleet = format!(
            "F:{}on{}off,h{},ag{},al{}",
            online,
            offline,
            pct(self.overall.score),
            self.overall.agent_count,
            self.overall.active_alerts,
        );
        parts.push(fleet);

        // Machine section
        if !self.machines.is_empty() {
            let machines: Vec<String> = self
                .machines
                .iter()
                .map(|m| machine_health_toon(m))
                .collect();
            parts.push(format!("M:{}", machines.join(",")));
        }

        // Alerts section
        let al = &self.alerts_by_severity;
        if al.critical > 0 || al.warning > 0 || al.info > 0 {
            parts.push(format!("AL:{}c{}w{}i", al.critical, al.warning, al.info));
        }

        parts.join("|")
    }
}

/// Convert TriageData to TOON format
///
/// Example output:
/// ```text
/// TOON1|TR:0recs|CMD:vc collect(90%)
/// ```
impl ToToon for TriageData {
    fn to_toon(&self) -> String {
        let mut parts = vec!["TOON1".to_string()];

        // Triage recommendations
        if self.recommendations.is_empty() {
            parts.push("TR:0recs".to_string());
        } else {
            let recs: Vec<String> = self
                .recommendations
                .iter()
                .map(|r| format!("p{}:{}", r.priority, abbreviate(&r.title, 30)))
                .collect();
            parts.push(format!("TR:{}", recs.join(",")));
        }

        // Suggested commands
        if !self.suggested_commands.is_empty() {
            let cmds: Vec<String> = self
                .suggested_commands
                .iter()
                .map(|c| format!("{}({}%)", abbreviate(&c.command, 20), pct(c.confidence)))
                .collect();
            parts.push(format!("CMD:{}", cmds.join(",")));
        }

        parts.join("|")
    }
}

/// Convert StatusData to TOON format
///
/// Example output:
/// ```text
/// TOON1|F:4on1off,h85|M:orko:on,h91,cpu45,mem68|RP:15t2d3a1b|AL:0c1h2m0l
/// ```
impl ToToon for StatusData {
    fn to_toon(&self) -> String {
        let mut parts = vec!["TOON1".to_string()];

        // Fleet summary
        let f = &self.fleet;
        parts.push(format!(
            "F:{}on{}off,h{}",
            f.online,
            f.offline,
            pct(f.health_score)
        ));

        // Machines
        if !self.machines.is_empty() {
            let machines: Vec<String> = self
                .machines
                .iter()
                .map(|m| {
                    let mut s = format!(
                        "{}:{},h{}",
                        abbreviate(&m.id, 12),
                        status_abbrev(&m.status),
                        pct(m.health_score)
                    );
                    if let Some(ref metrics) = m.metrics {
                        s.push_str(&format!(
                            ",cpu{},mem{}",
                            metrics.cpu_pct.round() as u32,
                            metrics.mem_pct.round() as u32
                        ));
                    }
                    if let Some(ref issue) = m.top_issue {
                        s.push_str(&format!(",!{}", abbreviate(issue, 15)));
                    }
                    s
                })
                .collect();
            parts.push(format!("M:{}", machines.join(",")));
        }

        // Repos
        let r = &self.repos;
        if r.total > 0 {
            parts.push(format!(
                "RP:{}t{}d{}a{}b",
                r.total, r.dirty, r.ahead, r.behind
            ));
        }

        // Alerts
        let a = &self.alerts;
        if a.critical > 0 || a.high > 0 || a.medium > 0 || a.low > 0 {
            parts.push(format!(
                "AL:{}c{}h{}m{}l",
                a.critical, a.high, a.medium, a.low
            ));
        }

        parts.join("|")
    }
}

/// Generic TOON for serde_json::Value — produces a compact key:value summary
impl ToToon for serde_json::Value {
    fn to_toon(&self) -> String {
        let mut parts = vec!["TOON1".to_string()];

        match self {
            serde_json::Value::Object(map) => {
                let items: Vec<String> = map
                    .iter()
                    .map(|(k, v)| format!("{}:{}", abbreviate(k, 12), value_toon(v)))
                    .collect();
                parts.push(format!("D:{}", items.join(",")));
            }
            serde_json::Value::Array(arr) => {
                parts.push(format!("A:{}items", arr.len()));
            }
            other => {
                parts.push(format!("V:{}", value_toon(other)));
            }
        }

        parts.join("|")
    }
}

/// Format any Serialize type as TOON by going through JSON first
pub fn to_toon_via_json<T: Serialize>(value: &T) -> String {
    match serde_json::to_value(value) {
        Ok(json_val) => json_val.to_toon(),
        Err(e) => format!("TOON1|ERR:{e}"),
    }
}

// ============================================================================
// Helper functions
// ============================================================================

/// Convert a 0.0-1.0 score to a percentage integer (0-100)
fn pct(score: f64) -> u32 {
    (score * 100.0).round() as u32
}

/// Abbreviate a status string
fn status_abbrev(status: &str) -> &str {
    match status {
        "online" => "on",
        "offline" => "off",
        "degraded" => "deg",
        "unknown" => "unk",
        other => other,
    }
}

/// Abbreviate a string to max length, appending ".." if truncated
fn abbreviate(s: &str, max_len: usize) -> String {
    if s.len() <= max_len {
        s.to_string()
    } else if max_len <= 2 {
        s[..max_len].to_string()
    } else {
        format!("{}..", &s[..max_len - 2])
    }
}

/// Convert a MachineHealth to a compact TOON segment
fn machine_health_toon(m: &MachineHealth) -> String {
    let mut s = format!(
        "{}:{},h{},{}ag",
        abbreviate(&m.id, 12),
        status_abbrev(&m.status),
        pct(m.score),
        m.agent_count
    );
    if let Some(cpu) = m.cpu_percent {
        s.push_str(&format!(",cpu{}", cpu.round() as u32));
    }
    if let Some(mem) = m.memory_percent {
        s.push_str(&format!(",mem{}", mem.round() as u32));
    }
    if let Some(ref issue) = m.top_issue {
        s.push_str(&format!(",!{}", abbreviate(issue, 15)));
    }
    s
}

/// Compact TOON representation of a JSON value
fn value_toon(v: &serde_json::Value) -> String {
    match v {
        serde_json::Value::Null => "-".to_string(),
        serde_json::Value::Bool(b) => if *b { "T" } else { "F" }.to_string(),
        serde_json::Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                i.to_string()
            } else if let Some(f) = n.as_f64() {
                format!("{:.1}", f)
            } else {
                n.to_string()
            }
        }
        serde_json::Value::String(s) => abbreviate(s, 25),
        serde_json::Value::Array(arr) => format!("[{}]", arr.len()),
        serde_json::Value::Object(map) => format!("{{{}}}", map.len()),
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::robot::*;
    use chrono::Utc;

    #[test]
    fn test_health_data_toon() {
        let health = HealthData {
            overall: OverallHealth {
                score: 0.85,
                severity: "warning".to_string(),
                active_alerts: 2,
                machine_count: 3,
                agent_count: 15,
            },
            machines: vec![
                MachineHealth {
                    id: "orko".to_string(),
                    name: "Orko".to_string(),
                    score: 0.91,
                    status: "online".to_string(),
                    top_issue: None,
                    last_seen: Utc::now(),
                    agent_count: 15,
                    cpu_percent: Some(45.0),
                    memory_percent: Some(68.0),
                },
                MachineHealth {
                    id: "backup".to_string(),
                    name: "Backup".to_string(),
                    score: 0.0,
                    status: "offline".to_string(),
                    top_issue: Some("no_response".to_string()),
                    last_seen: Utc::now(),
                    agent_count: 0,
                    cpu_percent: None,
                    memory_percent: None,
                },
            ],
            alerts_by_severity: AlertCounts {
                critical: 0,
                warning: 2,
                info: 1,
            },
        };

        let toon = health.to_toon();
        assert!(toon.starts_with("TOON1|"));
        assert!(toon.contains("F:1on1off"));
        assert!(toon.contains("h85"));
        assert!(toon.contains("ag15"));
        assert!(toon.contains("al2"));
        assert!(toon.contains("M:"));
        assert!(toon.contains("orko:on"));
        assert!(toon.contains("cpu45"));
        assert!(toon.contains("mem68"));
        assert!(toon.contains("backup:off"));
        assert!(toon.contains("!no_response"));
        assert!(toon.contains("AL:0c2w1i"));

        // Verify significant token reduction
        let json = serde_json::to_string(&health).unwrap();
        assert!(
            toon.len() < json.len() / 2,
            "TOON ({} bytes) should be less than half of JSON ({} bytes)",
            toon.len(),
            json.len()
        );
    }

    #[test]
    fn test_triage_data_toon() {
        let triage = TriageData {
            recommendations: vec![Recommendation {
                id: "rec-1".to_string(),
                priority: 1,
                title: "Fix rate limit exhaustion".to_string(),
                description: "Account near limit".to_string(),
                scope: "account".to_string(),
                action: "swap".to_string(),
            }],
            suggested_commands: vec![SuggestedCommand {
                command: "vc collect".to_string(),
                reason: "Run initial data collection".to_string(),
                confidence: 0.9,
            }],
        };

        let toon = triage.to_toon();
        assert!(toon.starts_with("TOON1|"));
        assert!(toon.contains("TR:"));
        assert!(toon.contains("p1:Fix rate limit exhaustion"));
        assert!(toon.contains("CMD:"));
        assert!(toon.contains("vc collect(90%)"));
    }

    #[test]
    fn test_triage_empty_toon() {
        let triage = TriageData {
            recommendations: vec![],
            suggested_commands: vec![],
        };

        let toon = triage.to_toon();
        assert!(toon.starts_with("TOON1|"));
        assert!(toon.contains("TR:0recs"));
    }

    #[test]
    fn test_status_data_toon() {
        let status = StatusData {
            fleet: FleetSummary {
                total_machines: 4,
                online: 3,
                offline: 1,
                health_score: 0.85,
            },
            machines: vec![MachineStatus {
                id: "orko".to_string(),
                status: "online".to_string(),
                last_seen: Utc::now(),
                health_score: 0.91,
                metrics: Some(MachineMetrics {
                    cpu_pct: 45.2,
                    mem_pct: 68.0,
                    load5: 1.8,
                    disk_free_pct: 35.0,
                }),
                top_issue: None,
            }],
            repos: RepoSummary {
                total: 15,
                dirty: 2,
                ahead: 3,
                behind: 1,
            },
            alerts: AlertSummary {
                critical: 0,
                high: 1,
                medium: 2,
                low: 0,
            },
        };

        let toon = status.to_toon();
        assert!(toon.starts_with("TOON1|"));
        assert!(toon.contains("F:3on1off,h85"));
        assert!(toon.contains("M:orko:on,h91,cpu45,mem68"));
        assert!(toon.contains("RP:15t2d3a1b"));
        assert!(toon.contains("AL:0c1h2m0l"));

        // Verify significant token reduction
        let json = serde_json::to_string(&status).unwrap();
        assert!(
            toon.len() < json.len() / 2,
            "TOON ({} bytes) should be less than half of JSON ({} bytes)",
            toon.len(),
            json.len()
        );
    }

    #[test]
    fn test_status_no_alerts_toon() {
        let status = StatusData {
            fleet: FleetSummary {
                total_machines: 1,
                online: 1,
                offline: 0,
                health_score: 1.0,
            },
            machines: vec![],
            repos: RepoSummary::default(),
            alerts: AlertSummary::default(),
        };

        let toon = status.to_toon();
        // No AL section when all zero
        assert!(!toon.contains("AL:"));
        // No RP section when total is 0
        assert!(!toon.contains("RP:"));
    }

    #[test]
    fn test_pct_helper() {
        assert_eq!(pct(0.0), 0);
        assert_eq!(pct(0.5), 50);
        assert_eq!(pct(1.0), 100);
        assert_eq!(pct(0.85), 85);
        assert_eq!(pct(0.999), 100); // rounds
    }

    #[test]
    fn test_abbreviate() {
        assert_eq!(abbreviate("short", 10), "short");
        assert_eq!(abbreviate("exactly-ten", 11), "exactly-ten");
        assert_eq!(abbreviate("this is a long string", 10), "this is ..");
        assert_eq!(abbreviate("ab", 2), "ab");
        assert_eq!(abbreviate("abc", 2), "ab");
    }

    #[test]
    fn test_status_abbrev() {
        assert_eq!(status_abbrev("online"), "on");
        assert_eq!(status_abbrev("offline"), "off");
        assert_eq!(status_abbrev("degraded"), "deg");
        assert_eq!(status_abbrev("unknown"), "unk");
        assert_eq!(status_abbrev("custom"), "custom");
    }

    #[test]
    fn test_json_value_toon() {
        let val = serde_json::json!({
            "name": "test",
            "count": 42,
            "active": true
        });
        let toon = val.to_toon();
        assert!(toon.starts_with("TOON1|"));
        assert!(toon.contains("D:"));
        assert!(toon.contains("name:test"));
        assert!(toon.contains("count:42"));
        assert!(toon.contains("active:T"));
    }

    #[test]
    fn test_to_toon_via_json() {
        let data = serde_json::json!({"key": "value"});
        let toon = to_toon_via_json(&data);
        assert!(toon.starts_with("TOON1|"));
        assert!(toon.contains("key:value"));
    }

    #[test]
    fn test_value_toon_types() {
        assert_eq!(value_toon(&serde_json::Value::Null), "-");
        assert_eq!(value_toon(&serde_json::json!(true)), "T");
        assert_eq!(value_toon(&serde_json::json!(false)), "F");
        assert_eq!(value_toon(&serde_json::json!(42)), "42");
        assert_eq!(value_toon(&serde_json::json!(3.14)), "3.1");
        assert_eq!(value_toon(&serde_json::json!("hello")), "hello");
        assert_eq!(value_toon(&serde_json::json!([1, 2, 3])), "[3]");
        assert_eq!(value_toon(&serde_json::json!({"a": 1})), "{1}");
    }
}
