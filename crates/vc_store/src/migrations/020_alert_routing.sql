-- Alert routing events: audit trail of routing decisions
CREATE TABLE IF NOT EXISTS alert_routing_events (
    id INTEGER PRIMARY KEY,
    alert_id TEXT NOT NULL,
    routed_at TIMESTAMP DEFAULT current_timestamp,
    rule_id TEXT,
    channel TEXT NOT NULL,
    action TEXT NOT NULL,
    reason_json TEXT
);

CREATE INDEX IF NOT EXISTS idx_alert_routing_alert ON alert_routing_events(alert_id);
CREATE INDEX IF NOT EXISTS idx_alert_routing_action ON alert_routing_events(action);
CREATE INDEX IF NOT EXISTS idx_alert_routing_routed_at ON alert_routing_events(routed_at);
