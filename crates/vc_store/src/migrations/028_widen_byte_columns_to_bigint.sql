-- Widen every byte-valued column from INTEGER to BIGINT.
--
-- DuckDB's INTEGER is INT32, so it tops out at 2,147,483,647 — about 2.1 GB.
-- Every column below stores a byte count, which means memory telemetry could
-- not be recorded for a machine with more than ~2 GB of RAM, and filesystem
-- telemetry could not be recorded for a disk larger than ~2 GB. In practice
-- that is every machine anyone would run this on: the insert fails outright
-- with "Conversion Error: Type INT64 with value 16000000000 can't be cast
-- because the value is out of range".
--
-- This went unnoticed because the collector logs an insert failure and carries
-- on (collectors are fail-soft by design), so the symptom was silently missing
-- memory and disk rows rather than a crash.
--
-- DuckDB refuses to change a column's type while an index depends on the table
-- ("Dependency Error: Cannot alter entry "sys_samples" because there are
-- entries that depend on it"), so the indexes on the altered tables are dropped
-- first and recreated unchanged at the end.

DROP INDEX IF EXISTS idx_sys_samples_ts;
DROP INDEX IF EXISTS idx_sys_samples_machine;
DROP INDEX IF EXISTS idx_pt_snapshots_machine;
DROP INDEX IF EXISTS idx_collector_health_machine;
DROP INDEX IF EXISTS idx_collector_health_ts;
DROP INDEX IF EXISTS idx_redaction_machine;
DROP INDEX IF EXISTS idx_redaction_collected;

ALTER TABLE sys_samples ALTER COLUMN mem_used_bytes TYPE BIGINT;
ALTER TABLE sys_samples ALTER COLUMN mem_total_bytes TYPE BIGINT;
ALTER TABLE sys_samples ALTER COLUMN mem_available_bytes TYPE BIGINT;
ALTER TABLE sys_samples ALTER COLUMN swap_used_bytes TYPE BIGINT;
ALTER TABLE sys_samples ALTER COLUMN swap_total_bytes TYPE BIGINT;

ALTER TABLE sys_fallback_samples ALTER COLUMN mem_total_bytes TYPE BIGINT;
ALTER TABLE sys_fallback_samples ALTER COLUMN mem_available_bytes TYPE BIGINT;
ALTER TABLE sys_fallback_samples ALTER COLUMN mem_used_bytes TYPE BIGINT;
ALTER TABLE sys_fallback_samples ALTER COLUMN swap_total_bytes TYPE BIGINT;
ALTER TABLE sys_fallback_samples ALTER COLUMN swap_used_bytes TYPE BIGINT;

ALTER TABLE sys_filesystems ALTER COLUMN total_bytes TYPE BIGINT;
ALTER TABLE sys_filesystems ALTER COLUMN used_bytes TYPE BIGINT;

ALTER TABLE sys_top_processes ALTER COLUMN mem_bytes TYPE BIGINT;
ALTER TABLE sys_top_processes ALTER COLUMN io_read_bytes TYPE BIGINT;
ALTER TABLE sys_top_processes ALTER COLUMN io_write_bytes TYPE BIGINT;

ALTER TABLE process_triage ALTER COLUMN mem_bytes TYPE BIGINT;

ALTER TABLE pt_snapshots ALTER COLUMN io_read_bytes TYPE BIGINT;
ALTER TABLE pt_snapshots ALTER COLUMN io_write_bytes TYPE BIGINT;

ALTER TABLE cass_index_status ALTER COLUMN index_size_bytes TYPE BIGINT;

ALTER TABLE collector_health ALTER COLUMN bytes_parsed TYPE BIGINT;

ALTER TABLE redaction_events ALTER COLUMN redacted_bytes TYPE BIGINT;

CREATE INDEX IF NOT EXISTS idx_sys_samples_ts ON sys_samples(collected_at);
CREATE INDEX IF NOT EXISTS idx_sys_samples_machine ON sys_samples(machine_id);
CREATE INDEX IF NOT EXISTS idx_pt_snapshots_machine ON pt_snapshots(machine_id);
CREATE INDEX IF NOT EXISTS idx_collector_health_machine
    ON collector_health(machine_id, collector);
CREATE INDEX IF NOT EXISTS idx_collector_health_ts
    ON collector_health(collected_at);
CREATE INDEX IF NOT EXISTS idx_redaction_machine
    ON redaction_events(machine_id, collector);
CREATE INDEX IF NOT EXISTS idx_redaction_collected
    ON redaction_events(collected_at);
