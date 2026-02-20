//! Watch mode: real-time JSONL event streaming for guardian agents.
//!
//! Emits structured events (alerts, predictions, health changes, collector status)
//! on stdout as newline-delimited JSON. Supports filtering by event type, machine,
//! and severity threshold.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

/// Severity levels for watch events, ordered lowest to highest.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum WatchSeverity {
    Low,
    Medium,
    High,
    Critical,
}

impl WatchSeverity {
    pub fn from_str_loose(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "low" | "l" => Some(Self::Low),
            "medium" | "med" | "m" => Some(Self::Medium),
            "high" | "h" => Some(Self::High),
            "critical" | "crit" | "c" => Some(Self::Critical),
            _ => None,
        }
    }
}

impl std::fmt::Display for WatchSeverity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Low => write!(f, "low"),
            Self::Medium => write!(f, "medium"),
            Self::High => write!(f, "high"),
            Self::Critical => write!(f, "critical"),
        }
    }
}

/// Event types emitted by the watch stream.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WatchEventType {
    Alert,
    Prediction,
    Opportunity,
    HealthChange,
    CollectorStatus,
    Heartbeat,
}

impl WatchEventType {
    pub fn from_str_loose(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "alert" => Some(Self::Alert),
            "prediction" => Some(Self::Prediction),
            "opportunity" => Some(Self::Opportunity),
            "health_change" | "healthchange" | "health" => Some(Self::HealthChange),
            "collector_status" | "collectorstatus" | "collector" => Some(Self::CollectorStatus),
            "heartbeat" => Some(Self::Heartbeat),
            _ => None,
        }
    }
}

impl std::fmt::Display for WatchEventType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Alert => write!(f, "alert"),
            Self::Prediction => write!(f, "prediction"),
            Self::Opportunity => write!(f, "opportunity"),
            Self::HealthChange => write!(f, "health_change"),
            Self::CollectorStatus => write!(f, "collector_status"),
            Self::Heartbeat => write!(f, "heartbeat"),
        }
    }
}

/// A single watch event, serialized as one JSONL line.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WatchEvent {
    #[serde(rename = "type")]
    pub event_type: WatchEventType,
    pub ts: DateTime<Utc>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub machine: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub severity: Option<WatchSeverity>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    /// Extra payload varies per event type.
    #[serde(flatten)]
    pub extra: serde_json::Value,
}

impl WatchEvent {
    /// Create an alert event.
    pub fn alert(machine: &str, severity: WatchSeverity, alert_id: &str, message: &str) -> Self {
        Self {
            event_type: WatchEventType::Alert,
            ts: Utc::now(),
            machine: Some(machine.to_string()),
            severity: Some(severity),
            message: Some(message.to_string()),
            extra: serde_json::json!({ "alert_id": alert_id }),
        }
    }

    /// Create a prediction event.
    pub fn prediction(
        machine: &str,
        prediction_type: &str,
        confidence: f64,
        action: &str,
    ) -> Self {
        Self {
            event_type: WatchEventType::Prediction,
            ts: Utc::now(),
            machine: Some(machine.to_string()),
            severity: None,
            message: None,
            extra: serde_json::json!({
                "prediction_type": prediction_type,
                "confidence": confidence,
                "action": action,
            }),
        }
    }

    /// Create a health change event.
    pub fn health_change(
        machine: &str,
        old_score: f64,
        new_score: f64,
        factor: &str,
    ) -> Self {
        let severity = if new_score < 0.5 {
            Some(WatchSeverity::Critical)
        } else if new_score < 0.7 {
            Some(WatchSeverity::High)
        } else if new_score < 0.85 {
            Some(WatchSeverity::Medium)
        } else {
            Some(WatchSeverity::Low)
        };
        Self {
            event_type: WatchEventType::HealthChange,
            ts: Utc::now(),
            machine: Some(machine.to_string()),
            severity,
            message: None,
            extra: serde_json::json!({
                "old_score": old_score,
                "new_score": new_score,
                "factor": factor,
            }),
        }
    }

    /// Create a collector status event.
    pub fn collector_status(
        machine: &str,
        collector: &str,
        status: &str,
        duration_ms: u64,
    ) -> Self {
        Self {
            event_type: WatchEventType::CollectorStatus,
            ts: Utc::now(),
            machine: Some(machine.to_string()),
            severity: None,
            message: None,
            extra: serde_json::json!({
                "collector": collector,
                "status": status,
                "duration_ms": duration_ms,
            }),
        }
    }

    /// Create an opportunity event.
    pub fn opportunity(
        opportunity_type: &str,
        estimated_savings: f64,
        action: &str,
    ) -> Self {
        Self {
            event_type: WatchEventType::Opportunity,
            ts: Utc::now(),
            machine: None,
            severity: None,
            message: None,
            extra: serde_json::json!({
                "opportunity_type": opportunity_type,
                "estimated_savings": estimated_savings,
                "action": action,
            }),
        }
    }

    /// Create a heartbeat event.
    pub fn heartbeat() -> Self {
        Self {
            event_type: WatchEventType::Heartbeat,
            ts: Utc::now(),
            machine: None,
            severity: None,
            message: Some("heartbeat".to_string()),
            extra: serde_json::Value::Object(serde_json::Map::new()),
        }
    }

    /// Serialize to a single JSONL line.
    pub fn to_jsonl(&self) -> String {
        serde_json::to_string(self).unwrap_or_else(|_| "{}".to_string())
    }

    /// Serialize to TOON format.
    pub fn to_toon(&self) -> String {
        let ty = match self.event_type {
            WatchEventType::Alert => "AL",
            WatchEventType::Prediction => "PR",
            WatchEventType::Opportunity => "OP",
            WatchEventType::HealthChange => "HC",
            WatchEventType::CollectorStatus => "CS",
            WatchEventType::Heartbeat => "HB",
        };
        let sev = self
            .severity
            .as_ref()
            .map(|s| format!(",{s}"))
            .unwrap_or_default();
        let mach = self
            .machine
            .as_deref()
            .map(|m| format!(",{m}"))
            .unwrap_or_default();
        let msg = self
            .message
            .as_deref()
            .map(|m| {
                let truncated = if m.len() > 40 {
                    format!("{}..", &m[..38])
                } else {
                    m.to_string()
                };
                format!(",{truncated}")
            })
            .unwrap_or_default();
        format!("W|{ty}{sev}{mach}{msg}")
    }
}

/// Filter configuration for the watch stream.
#[derive(Debug, Clone)]
pub struct WatchFilter {
    pub event_types: Option<HashSet<WatchEventType>>,
    pub machines: Option<HashSet<String>>,
    pub min_severity: Option<WatchSeverity>,
}

impl WatchFilter {
    /// Parse event type strings into a filter set.
    pub fn parse_event_types(events: &[String]) -> Option<HashSet<WatchEventType>> {
        let set: HashSet<WatchEventType> = events
            .iter()
            .filter_map(|s| WatchEventType::from_str_loose(s))
            .collect();
        if set.is_empty() { None } else { Some(set) }
    }

    /// Parse machine name strings into a filter set.
    pub fn parse_machines(machines: &[String]) -> Option<HashSet<String>> {
        let set: HashSet<String> = machines
            .iter()
            .map(|s| s.to_lowercase())
            .collect();
        if set.is_empty() { None } else { Some(set) }
    }

    /// Check whether a given event passes this filter.
    pub fn matches(&self, event: &WatchEvent) -> bool {
        // Heartbeats always pass
        if event.event_type == WatchEventType::Heartbeat {
            return true;
        }

        // Event type filter
        if let Some(ref types) = self.event_types {
            if !types.contains(&event.event_type) {
                return false;
            }
        }

        // Machine filter
        if let Some(ref machines) = self.machines {
            if let Some(ref machine) = event.machine {
                if !machines.contains(&machine.to_lowercase()) {
                    return false;
                }
            }
            // Events without a machine field pass the machine filter
        }

        // Severity filter
        if let Some(ref min_sev) = self.min_severity {
            if let Some(ref sev) = event.severity {
                if sev < min_sev {
                    return false;
                }
            }
            // Events without a severity field pass the severity filter
        }

        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_watch_severity_ordering() {
        assert!(WatchSeverity::Low < WatchSeverity::Medium);
        assert!(WatchSeverity::Medium < WatchSeverity::High);
        assert!(WatchSeverity::High < WatchSeverity::Critical);
    }

    #[test]
    fn test_watch_severity_from_str() {
        assert_eq!(WatchSeverity::from_str_loose("low"), Some(WatchSeverity::Low));
        assert_eq!(WatchSeverity::from_str_loose("HIGH"), Some(WatchSeverity::High));
        assert_eq!(WatchSeverity::from_str_loose("crit"), Some(WatchSeverity::Critical));
        assert_eq!(WatchSeverity::from_str_loose("med"), Some(WatchSeverity::Medium));
        assert_eq!(WatchSeverity::from_str_loose("bogus"), None);
    }

    #[test]
    fn test_watch_event_type_from_str() {
        assert_eq!(WatchEventType::from_str_loose("alert"), Some(WatchEventType::Alert));
        assert_eq!(WatchEventType::from_str_loose("health_change"), Some(WatchEventType::HealthChange));
        assert_eq!(WatchEventType::from_str_loose("health"), Some(WatchEventType::HealthChange));
        assert_eq!(WatchEventType::from_str_loose("collector"), Some(WatchEventType::CollectorStatus));
        assert_eq!(WatchEventType::from_str_loose("nope"), None);
    }

    #[test]
    fn test_alert_event_jsonl() {
        let event = WatchEvent::alert("orko", WatchSeverity::Critical, "a-123", "CPU spike");
        let jsonl = event.to_jsonl();
        assert!(jsonl.contains("\"type\":\"alert\""));
        assert!(jsonl.contains("\"machine\":\"orko\""));
        assert!(jsonl.contains("\"severity\":\"critical\""));
        assert!(jsonl.contains("\"alert_id\":\"a-123\""));
        assert!(jsonl.contains("\"message\":\"CPU spike\""));
    }

    #[test]
    fn test_prediction_event_jsonl() {
        let event = WatchEvent::prediction("orko", "rate_limit", 0.85, "swap_now");
        let jsonl = event.to_jsonl();
        assert!(jsonl.contains("\"type\":\"prediction\""));
        assert!(jsonl.contains("\"confidence\":0.85"));
        assert!(jsonl.contains("\"action\":\"swap_now\""));
    }

    #[test]
    fn test_health_change_severity_assignment() {
        // New score < 0.5 → critical
        let e = WatchEvent::health_change("m1", 0.9, 0.3, "cpu");
        assert_eq!(e.severity, Some(WatchSeverity::Critical));

        // New score < 0.7 → high
        let e = WatchEvent::health_change("m1", 0.9, 0.6, "mem");
        assert_eq!(e.severity, Some(WatchSeverity::High));

        // New score < 0.85 → medium
        let e = WatchEvent::health_change("m1", 0.9, 0.8, "disk");
        assert_eq!(e.severity, Some(WatchSeverity::Medium));

        // New score >= 0.85 → low
        let e = WatchEvent::health_change("m1", 0.8, 0.9, "recovery");
        assert_eq!(e.severity, Some(WatchSeverity::Low));
    }

    #[test]
    fn test_collector_status_event() {
        let event = WatchEvent::collector_status("orko", "sysmoni", "ok", 234);
        let jsonl = event.to_jsonl();
        assert!(jsonl.contains("\"collector_status\""));
        assert!(jsonl.contains("\"collector\":\"sysmoni\""));
        assert!(jsonl.contains("\"duration_ms\":234"));
    }

    #[test]
    fn test_opportunity_event() {
        let event = WatchEvent::opportunity("cost_saving", 12.50, "downgrade_plan");
        let jsonl = event.to_jsonl();
        assert!(jsonl.contains("\"opportunity\""));
        assert!(jsonl.contains("\"estimated_savings\":12.5"));
    }

    #[test]
    fn test_heartbeat_event() {
        let event = WatchEvent::heartbeat();
        let jsonl = event.to_jsonl();
        assert!(jsonl.contains("\"type\":\"heartbeat\""));
        assert!(jsonl.contains("\"message\":\"heartbeat\""));
    }

    #[test]
    fn test_event_toon_format() {
        let event = WatchEvent::alert("orko", WatchSeverity::High, "a-1", "disk full");
        let toon = event.to_toon();
        assert!(toon.starts_with("W|AL"));
        assert!(toon.contains("high"));
        assert!(toon.contains("orko"));
        assert!(toon.contains("disk full"));
    }

    #[test]
    fn test_toon_heartbeat() {
        let event = WatchEvent::heartbeat();
        let toon = event.to_toon();
        assert!(toon.starts_with("W|HB"));
        assert!(toon.contains("heartbeat"));
    }

    #[test]
    fn test_filter_event_type() {
        let mut types = HashSet::new();
        types.insert(WatchEventType::Alert);
        let filter = WatchFilter {
            event_types: Some(types),
            machines: None,
            min_severity: None,
        };
        let alert = WatchEvent::alert("m1", WatchSeverity::Low, "a1", "test");
        assert!(filter.matches(&alert));

        let prediction = WatchEvent::prediction("m1", "rate", 0.5, "wait");
        assert!(!filter.matches(&prediction));
    }

    #[test]
    fn test_filter_machine() {
        let machines: HashSet<String> = ["orko".to_string()].into();
        let filter = WatchFilter {
            event_types: None,
            machines: Some(machines),
            min_severity: None,
        };
        let orko_event = WatchEvent::alert("orko", WatchSeverity::Low, "a1", "test");
        assert!(filter.matches(&orko_event));

        let other_event = WatchEvent::alert("sydneymc", WatchSeverity::Low, "a2", "test");
        assert!(!filter.matches(&other_event));
    }

    #[test]
    fn test_filter_severity() {
        let filter = WatchFilter {
            event_types: None,
            machines: None,
            min_severity: Some(WatchSeverity::High),
        };
        let critical = WatchEvent::alert("m1", WatchSeverity::Critical, "a1", "bad");
        assert!(filter.matches(&critical));

        let high = WatchEvent::alert("m1", WatchSeverity::High, "a2", "bad");
        assert!(filter.matches(&high));

        let low = WatchEvent::alert("m1", WatchSeverity::Low, "a3", "meh");
        assert!(!filter.matches(&low));
    }

    #[test]
    fn test_heartbeat_always_passes_filter() {
        let mut types = HashSet::new();
        types.insert(WatchEventType::Alert);
        let filter = WatchFilter {
            event_types: Some(types),
            machines: Some(["orko".to_string()].into()),
            min_severity: Some(WatchSeverity::Critical),
        };
        let hb = WatchEvent::heartbeat();
        assert!(filter.matches(&hb));
    }

    #[test]
    fn test_parse_event_types() {
        let input = vec!["alert".to_string(), "health".to_string(), "bogus".to_string()];
        let result = WatchFilter::parse_event_types(&input).unwrap();
        assert!(result.contains(&WatchEventType::Alert));
        assert!(result.contains(&WatchEventType::HealthChange));
        assert_eq!(result.len(), 2);
    }

    #[test]
    fn test_parse_event_types_empty() {
        let input = vec!["bogus".to_string()];
        assert!(WatchFilter::parse_event_types(&input).is_none());
    }

    #[test]
    fn test_parse_machines() {
        let input = vec!["Orko".to_string(), "SydneyMC".to_string()];
        let result = WatchFilter::parse_machines(&input).unwrap();
        assert!(result.contains("orko"));
        assert!(result.contains("sydneymc"));
    }

    #[test]
    fn test_combined_filter() {
        let mut types = HashSet::new();
        types.insert(WatchEventType::Alert);
        types.insert(WatchEventType::HealthChange);
        let filter = WatchFilter {
            event_types: Some(types),
            machines: Some(["orko".to_string()].into()),
            min_severity: Some(WatchSeverity::Medium),
        };

        // Matching: alert on orko, high severity
        let good = WatchEvent::alert("orko", WatchSeverity::High, "a1", "disk");
        assert!(filter.matches(&good));

        // Wrong machine
        let wrong_machine = WatchEvent::alert("sydneymc", WatchSeverity::High, "a2", "disk");
        assert!(!filter.matches(&wrong_machine));

        // Wrong type
        let wrong_type = WatchEvent::prediction("orko", "rate", 0.9, "swap");
        assert!(!filter.matches(&wrong_type));

        // Too low severity
        let low_sev = WatchEvent::alert("orko", WatchSeverity::Low, "a3", "meh");
        assert!(!filter.matches(&low_sev));
    }

    #[test]
    fn test_toon_long_message_truncation() {
        let event = WatchEvent::alert(
            "orko",
            WatchSeverity::Critical,
            "a-1",
            "This is a very long message that exceeds the forty character limit for toon output",
        );
        let toon = event.to_toon();
        // Message should be truncated with ".." suffix
        assert!(toon.len() < 200);
        assert!(toon.contains(".."));
    }

    #[test]
    fn test_event_roundtrip_serde() {
        let event = WatchEvent::alert("orko", WatchSeverity::Critical, "a-123", "test alert");
        let json = serde_json::to_string(&event).unwrap();
        let parsed: WatchEvent = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.event_type, WatchEventType::Alert);
        assert_eq!(parsed.severity, Some(WatchSeverity::Critical));
        assert_eq!(parsed.machine.as_deref(), Some("orko"));
    }
}
