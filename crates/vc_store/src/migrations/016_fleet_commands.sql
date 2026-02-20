-- Fleet orchestration command history
-- Tracks spawn, rebalance, emergency-stop, and migrate operations

CREATE TABLE IF NOT EXISTS fleet_commands (
    command_id TEXT PRIMARY KEY,
    command_type TEXT NOT NULL,        -- spawn, rebalance, emergency_stop, migrate
    params_json TEXT NOT NULL,         -- JSON of command parameters
    status TEXT NOT NULL DEFAULT 'pending',  -- pending, running, completed, failed
    started_at TIMESTAMP DEFAULT current_timestamp,
    completed_at TIMESTAMP,
    result_json TEXT,                  -- JSON of command results
    error_message TEXT,
    initiated_by TEXT                  -- agent name or 'user'
);

CREATE INDEX IF NOT EXISTS idx_fleet_cmd_type ON fleet_commands(command_type);
CREATE INDEX IF NOT EXISTS idx_fleet_cmd_status ON fleet_commands(status);
CREATE INDEX IF NOT EXISTS idx_fleet_cmd_started ON fleet_commands(started_at);
