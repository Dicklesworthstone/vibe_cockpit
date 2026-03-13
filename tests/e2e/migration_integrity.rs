#[path = "../common/mod.rs"]
mod common;

use chrono::{DateTime, Duration as ChronoDuration, SecondsFormat, Utc};
use duckdb::{
    Connection as DuckConnection,
    types::{TimeUnit as DuckTimeUnit, Value as DuckValue},
};
use fsqlite::{Connection as FrankenConnection, SqliteValue};
use serde_json::Value;
use std::collections::BTreeSet;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::process::Command;
use tempfile::tempdir;

#[derive(Clone, Copy)]
struct TableSpec {
    name: &'static str,
    order_by: &'static str,
    expected_rows: usize,
    json_columns: &'static [usize],
}

const TABLE_SPECS: [TableSpec; 3] = [
    TableSpec {
        name: "machines",
        order_by: "id",
        expected_rows: 50,
        json_columns: &[],
    },
    TableSpec {
        name: "agent_sessions",
        order_by: "session_id",
        expected_rows: 1_000,
        json_columns: &[5, 6],
    },
    TableSpec {
        name: "system_metrics",
        order_by: "machine_id, collected_at",
        expected_rows: 1_000,
        json_columns: &[5, 6],
    },
];

#[test]
fn migration_integrity_matches_representative_source_data() {
    common::init_tracing();

    let dir = tempdir().unwrap();
    let source_path = dir.path().join("source.duckdb");
    let target_path = dir.path().join("target.sqlite");

    let source = DuckConnection::open(&source_path).unwrap();
    create_source_fixture(&source);
    drop(source);

    let output = Command::new(env!("CARGO_BIN_EXE_vc"))
        .args([
            "--format",
            "json",
            "migrate-db",
            "--from",
            source_path.to_string_lossy().as_ref(),
            "--to",
            target_path.to_string_lossy().as_ref(),
        ])
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "migration command failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(target_path.exists(), "target database was not created");
    assert!(stderr.contains("Migrating machines: 50 rows..."));
    assert!(stderr.contains("Migrating agent_sessions: 1000 rows..."));
    assert!(stderr.contains("Migrating system_metrics: 1000 rows..."));

    let source = DuckConnection::open(&source_path).unwrap();
    let target = FrankenConnection::open(target_path.to_string_lossy().as_ref()).unwrap();

    for spec in TABLE_SPECS {
        assert_table_integrity(&source, &target, spec);
    }
}

fn create_source_fixture(source: &DuckConnection) {
    source
        .execute_batch(
            r"
            CREATE TABLE machines (
                id INTEGER PRIMARY KEY,
                hostname VARCHAR NOT NULL,
                enabled BOOLEAN NOT NULL,
                created_at TIMESTAMP NOT NULL,
                notes VARCHAR
            );

            CREATE TABLE agent_sessions (
                session_id BIGINT PRIMARY KEY,
                machine_id INTEGER NOT NULL,
                agent_name VARCHAR NOT NULL,
                started_at TIMESTAMP NOT NULL,
                token_count BIGINT NOT NULL,
                tags VARCHAR[] NOT NULL,
                metadata STRUCT(provider VARCHAR, model VARCHAR, tier VARCHAR) NOT NULL,
                notes VARCHAR,
                FOREIGN KEY(machine_id) REFERENCES machines(id)
            );

            CREATE TABLE system_metrics (
                machine_id INTEGER NOT NULL,
                collected_at TIMESTAMP NOT NULL,
                cpu_pct DOUBLE NOT NULL,
                mem_pct DOUBLE,
                notes VARCHAR,
                labels VARCHAR[] NOT NULL,
                details STRUCT(samples INTEGER, hot BOOLEAN) NOT NULL,
                PRIMARY KEY(machine_id, collected_at),
                FOREIGN KEY(machine_id) REFERENCES machines(id)
            );
            ",
        )
        .unwrap();

    let base = DateTime::parse_from_rfc3339("2026-01-01T00:00:00Z")
        .unwrap()
        .with_timezone(&Utc);

    let mut machine_sql = String::new();
    for id in 1..=50 {
        let created_at = base + ChronoDuration::minutes(i64::from(id) * 5);
        let notes = if id % 10 == 0 {
            "NULL".to_string()
        } else if id % 11 == 0 {
            sql_text("")
        } else if id % 13 == 0 {
            sql_text("Zoë")
        } else {
            sql_text(&format!("note-{id:03}"))
        };
        machine_sql.push_str(&format!(
            "INSERT INTO machines VALUES ({id}, {}, {}, {}, {});\n",
            sql_text(&format!("machine-{id:03}")),
            sql_bool(id % 2 == 0),
            sql_timestamp(created_at),
            notes,
        ));
    }
    source.execute_batch(&machine_sql).unwrap();

    let mut session_sql = String::new();
    for session_id in 1..=1_000_i64 {
        let machine_id = i32::try_from(((session_id - 1) % 50) + 1).unwrap();
        let started_at = base + ChronoDuration::seconds(session_id * 37);
        let agent_name = if session_id % 250 == 0 {
            "Zoë".to_string()
        } else if session_id % 333 == 0 {
            "李雷".to_string()
        } else {
            format!("agent-{session_id:04}")
        };
        let token_count = 1_000 + session_id * 17;
        let tags = if session_id % 5 == 0 {
            sql_list(&["alpha", "beta"])
        } else {
            sql_list(&["solo"])
        };
        let provider = if session_id % 2 == 0 {
            "openai"
        } else {
            "anthropic"
        };
        let model = if session_id % 3 == 0 {
            "gpt-5"
        } else {
            "claude-4"
        };
        let tier = if session_id % 7 == 0 {
            "backup"
        } else {
            "primary"
        };
        let metadata = format!(
            "struct_pack(provider := {}, model := {}, tier := {})",
            sql_text(provider),
            sql_text(model),
            sql_text(tier),
        );
        let notes = if session_id % 10 == 0 {
            "NULL".to_string()
        } else if session_id % 14 == 0 {
            sql_text("")
        } else {
            sql_text(&format!("session-{session_id:04}"))
        };
        session_sql.push_str(&format!(
            "INSERT INTO agent_sessions VALUES ({session_id}, {machine_id}, {}, {}, {token_count}, {tags}, {metadata}, {notes});\n",
            sql_text(&agent_name),
            sql_timestamp(started_at),
        ));
    }
    source.execute_batch(&session_sql).unwrap();

    let mut metric_sql = String::new();
    for index in 1..=1_000_i64 {
        let machine_id = i32::try_from(((index - 1) % 50) + 1).unwrap();
        let collected_at = base + ChronoDuration::seconds(index * 30);
        let cpu_pct = format!("{:.3}", f64::from(((index % 100) + 1) as i32) / 1.7);
        let mem_pct = if index % 9 == 0 {
            "NULL".to_string()
        } else {
            format!("{:.3}", f64::from(((index * 3) % 100) as i32) / 2.1)
        };
        let notes = if index % 13 == 0 {
            "NULL".to_string()
        } else if index % 17 == 0 {
            sql_text("")
        } else {
            sql_text(&format!("metric-{index:04}"))
        };
        let labels = if index % 4 == 0 {
            sql_list(&["cpu", "burst"])
        } else {
            sql_list(&["steady"])
        };
        let details = format!(
            "struct_pack(samples := {}, hot := {})",
            (index % 5) + 1,
            sql_bool(index % 3 == 0),
        );
        metric_sql.push_str(&format!(
            "INSERT INTO system_metrics VALUES ({machine_id}, {}, {cpu_pct}, {mem_pct}, {notes}, {labels}, {details});\n",
            sql_timestamp(collected_at),
        ));
    }
    source.execute_batch(&metric_sql).unwrap();
}

fn assert_table_integrity(source: &DuckConnection, target: &FrankenConnection, spec: TableSpec) {
    let source_rows = collect_duck_rows(
        source,
        &format!("SELECT * FROM \"{}\" ORDER BY {}", spec.name, spec.order_by),
    );
    let target_rows = collect_sqlite_rows(
        target,
        &format!("SELECT * FROM \"{}\" ORDER BY {}", spec.name, spec.order_by),
        spec.json_columns,
    );

    assert_eq!(
        source_rows.len(),
        spec.expected_rows,
        "unexpected source row count for {}",
        spec.name
    );
    assert_eq!(
        source_rows.len(),
        target_rows.len(),
        "row-count mismatch for {}",
        spec.name
    );

    let source_checksum = checksum_rows(&source_rows);
    let target_checksum = checksum_rows(&target_rows);
    eprintln!(
        "verified {} rows={} checksum={:#x}",
        spec.name,
        source_rows.len(),
        source_checksum
    );
    assert_eq!(
        source_checksum, target_checksum,
        "checksum mismatch for {}",
        spec.name
    );

    assert_eq!(
        null_counts(&source_rows),
        null_counts(&target_rows),
        "NULL-count mismatch for {}",
        spec.name
    );

    for index in sample_indexes(spec.name, source_rows.len()) {
        assert_eq!(
            source_rows[index], target_rows[index],
            "sample mismatch for {} at row index {}",
            spec.name, index
        );
    }
}

fn collect_duck_rows(source: &DuckConnection, sql: &str) -> Vec<Vec<Value>> {
    let mut stmt = source.prepare(sql).unwrap();
    let column_count = stmt.column_count();
    let mut rows = stmt.query([]).unwrap();
    let mut collected = Vec::new();
    while let Some(row) = rows.next().unwrap() {
        let mut values = Vec::with_capacity(column_count);
        for index in 0..column_count {
            values.push(normalize_duck_value(row.get_ref_unwrap(index).to_owned()));
        }
        collected.push(values);
    }
    collected
}

fn collect_sqlite_rows(
    target: &FrankenConnection,
    sql: &str,
    json_columns: &[usize],
) -> Vec<Vec<Value>> {
    let stmt = target.prepare(sql).unwrap();
    let rows = stmt.query().unwrap();
    rows.into_iter()
        .map(|row| {
            row.values()
                .iter()
                .enumerate()
                .map(|(index, value)| normalize_sqlite_value(value, json_columns.contains(&index)))
                .collect()
        })
        .collect()
}

fn normalize_duck_value(value: DuckValue) -> Value {
    match value {
        DuckValue::Null => Value::Null,
        DuckValue::Boolean(flag) => Value::from(i64::from(u8::from(flag))),
        DuckValue::TinyInt(number) => Value::from(i64::from(number)),
        DuckValue::SmallInt(number) => Value::from(i64::from(number)),
        DuckValue::Int(number) => Value::from(i64::from(number)),
        DuckValue::BigInt(number) => Value::from(number),
        DuckValue::UTinyInt(number) => Value::from(u64::from(number)),
        DuckValue::USmallInt(number) => Value::from(u64::from(number)),
        DuckValue::UInt(number) => Value::from(u64::from(number)),
        DuckValue::Float(number) => serde_json::json!(f64::from(number)),
        DuckValue::Double(number) => serde_json::json!(number),
        DuckValue::Text(text) => Value::String(text),
        DuckValue::Blob(bytes) => {
            Value::Array(bytes.into_iter().map(Value::from).collect::<Vec<_>>())
        }
        DuckValue::Timestamp(unit, value) => Value::String(format_timestamp(unit, value)),
        DuckValue::Date32(days) => Value::String(format_date(days)),
        DuckValue::Time64(unit, value) => Value::String(format_time(unit, value)),
        DuckValue::HugeInt(number) => Value::String(number.to_string()),
        DuckValue::UBigInt(number) => Value::String(number.to_string()),
        DuckValue::Decimal(decimal) => Value::String(decimal.normalize().to_string()),
        DuckValue::Enum(value) => Value::String(value),
        DuckValue::Interval {
            months,
            days,
            nanos,
        } => serde_json::json!({
            "months": months,
            "days": days,
            "nanos": nanos,
        }),
        DuckValue::List(values) | DuckValue::Array(values) => {
            Value::Array(values.into_iter().map(normalize_duck_value).collect())
        }
        DuckValue::Struct(fields) => {
            let mut object = serde_json::Map::new();
            for (key, value) in fields.iter() {
                object.insert(key.clone(), normalize_duck_value(value.clone()));
            }
            Value::Object(object)
        }
        DuckValue::Map(entries) => Value::Array(
            entries
                .iter()
                .map(|(key, value)| {
                    serde_json::json!({
                        "key": normalize_duck_value(key.clone()),
                        "value": normalize_duck_value(value.clone()),
                    })
                })
                .collect(),
        ),
        DuckValue::Union(value) => normalize_duck_value(*value),
    }
}

fn normalize_sqlite_value(value: &SqliteValue, parse_json: bool) -> Value {
    match value {
        SqliteValue::Null => Value::Null,
        SqliteValue::Integer(number) => Value::from(*number),
        SqliteValue::Float(number) => serde_json::json!(*number),
        SqliteValue::Text(text) if parse_json => serde_json::from_str(text).unwrap(),
        SqliteValue::Text(text) => Value::String(text.clone()),
        SqliteValue::Blob(bytes) => {
            Value::Array(bytes.iter().copied().map(Value::from).collect::<Vec<_>>())
        }
    }
}

fn checksum_rows(rows: &[Vec<Value>]) -> u64 {
    let mut hasher = DefaultHasher::new();
    serde_json::to_string(rows).unwrap().hash(&mut hasher);
    hasher.finish()
}

fn null_counts(rows: &[Vec<Value>]) -> Vec<usize> {
    let column_count = rows.first().map_or(0, Vec::len);
    let mut counts = vec![0_usize; column_count];
    for row in rows {
        for (index, value) in row.iter().enumerate() {
            if value.is_null() {
                counts[index] += 1;
            }
        }
    }
    counts
}

fn sample_indexes(table: &str, row_count: usize) -> Vec<usize> {
    if row_count == 0 {
        return Vec::new();
    }

    let mut indexes = BTreeSet::from([0_usize, row_count - 1]);
    let seed = table.bytes().fold(0_u64, |acc, byte| {
        acc.wrapping_mul(131).wrapping_add(u64::from(byte))
    });
    for salt in 0_u64..10 {
        let index = usize::try_from(
            (seed ^ salt.wrapping_mul(97).wrapping_add(17)) % u64::try_from(row_count).unwrap(),
        )
        .unwrap();
        indexes.insert(index);
    }
    indexes.into_iter().collect()
}

fn sql_text(value: &str) -> String {
    format!("'{}'", value.replace('\'', "''"))
}

fn sql_bool(value: bool) -> &'static str {
    if value { "TRUE" } else { "FALSE" }
}

fn sql_list(values: &[&str]) -> String {
    format!(
        "[{}]",
        values
            .iter()
            .map(|value| sql_text(value))
            .collect::<Vec<_>>()
            .join(", ")
    )
}

fn sql_timestamp(value: DateTime<Utc>) -> String {
    format!("TIMESTAMP '{}'", value.format("%Y-%m-%d %H:%M:%S%.6f"))
}

fn format_timestamp(unit: DuckTimeUnit, value: i64) -> String {
    let (seconds, nanos) = split_timestamp(unit, value);
    DateTime::<Utc>::from_timestamp(seconds, nanos)
        .unwrap()
        .to_rfc3339_opts(SecondsFormat::Micros, true)
}

fn split_timestamp(unit: DuckTimeUnit, value: i64) -> (i64, u32) {
    match unit {
        DuckTimeUnit::Second => (value, 0),
        DuckTimeUnit::Millisecond => {
            let seconds = value.div_euclid(1_000);
            let nanos = u32::try_from(value.rem_euclid(1_000)).unwrap() * 1_000_000;
            (seconds, nanos)
        }
        DuckTimeUnit::Microsecond => {
            let seconds = value.div_euclid(1_000_000);
            let nanos = u32::try_from(value.rem_euclid(1_000_000)).unwrap() * 1_000;
            (seconds, nanos)
        }
        DuckTimeUnit::Nanosecond => {
            let seconds = value.div_euclid(1_000_000_000);
            let nanos = u32::try_from(value.rem_euclid(1_000_000_000)).unwrap();
            (seconds, nanos)
        }
    }
}

fn format_date(days_since_epoch: i32) -> String {
    let epoch = DateTime::<Utc>::from_timestamp(0, 0).unwrap().date_naive();
    epoch
        .checked_add_signed(ChronoDuration::days(i64::from(days_since_epoch)))
        .unwrap()
        .format("%Y-%m-%d")
        .to_string()
}

fn format_time(unit: DuckTimeUnit, value: i64) -> String {
    let total_nanos = match unit {
        DuckTimeUnit::Second => i128::from(value) * 1_000_000_000,
        DuckTimeUnit::Millisecond => i128::from(value) * 1_000_000,
        DuckTimeUnit::Microsecond => i128::from(value) * 1_000,
        DuckTimeUnit::Nanosecond => i128::from(value),
    };
    let nanos_per_day = 86_400_i128 * 1_000_000_000;
    let normalized = total_nanos.rem_euclid(nanos_per_day);
    let seconds = normalized / 1_000_000_000;
    let nanos = u32::try_from(normalized % 1_000_000_000).unwrap();
    let hours = seconds / 3_600;
    let minutes = (seconds % 3_600) / 60;
    let whole_seconds = seconds % 60;

    if nanos == 0 {
        format!("{hours:02}:{minutes:02}:{whole_seconds:02}")
    } else {
        let mut fractional = format!("{nanos:09}");
        while fractional.ends_with('0') {
            fractional.pop();
        }
        format!("{hours:02}:{minutes:02}:{whole_seconds:02}.{fractional}")
    }
}
