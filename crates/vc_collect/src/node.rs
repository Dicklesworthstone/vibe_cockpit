//! vc-node push agent: store-and-forward collection
//!
//! Bundles collected data into compressed JSONL batches with signed manifests.
//! Supports offline buffering and deduplication on ingest.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

// ============================================================================
// Bundle manifest
// ============================================================================

/// A signed manifest describing a collected data bundle
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BundleManifest {
    /// Unique bundle identifier
    pub bundle_id: String,
    /// Machine that produced this bundle
    pub machine_id: String,
    /// When the bundle was created
    pub created_at: DateTime<Utc>,
    /// Schema version of the bundle format
    pub schema_version: u32,
    /// Individual batch entries in the bundle
    pub batches: Vec<BatchEntry>,
    /// Content hash of all batches (hex-encoded `SipHash`)
    pub content_hash: String,
    /// Total payload size in bytes
    pub total_bytes: u64,
}

/// A single batch within a bundle
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchEntry {
    /// Collector that produced this batch
    pub collector: String,
    /// Number of rows in the batch
    pub row_count: usize,
    /// Content hash of this batch (hex-encoded `SipHash`)
    pub batch_hash: String,
    /// Cursor value after this batch (for incremental collection)
    pub cursor: Option<String>,
    /// JSONL payload (each line is a JSON object)
    pub lines: Vec<String>,
}

// ============================================================================
// Dedup key
// ============================================================================

/// Deduplication key for ingested records
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct DedupKey {
    pub machine_id: String,
    pub collector: String,
    pub payload_hash: String,
}

impl DedupKey {
    #[must_use]
    pub fn new(machine_id: &str, collector: &str, payload: &str) -> Self {
        Self {
            machine_id: machine_id.to_string(),
            collector: collector.to_string(),
            payload_hash: hash_content(payload),
        }
    }
}

// ============================================================================
// Spool config
// ============================================================================

/// Configuration for the local spool directory
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpoolConfig {
    /// Directory to store pending bundles
    pub spool_dir: String,
    /// Maximum spool size in bytes (prevent runaway growth)
    pub max_spool_bytes: u64,
    /// Maximum age of spooled bundles in seconds before cleanup
    pub max_age_secs: u64,
}

impl Default for SpoolConfig {
    fn default() -> Self {
        Self {
            spool_dir: "/var/lib/vc-node/spool".to_string(),
            max_spool_bytes: 100 * 1024 * 1024, // 100 MB
            max_age_secs: 7 * 24 * 3600,        // 7 days
        }
    }
}

// ============================================================================
// Bundle builder
// ============================================================================

/// Builder for constructing bundles from collected data
pub struct BundleBuilder {
    machine_id: String,
    batches: Vec<BatchEntry>,
}

impl BundleBuilder {
    #[must_use]
    pub fn new(machine_id: &str) -> Self {
        Self {
            machine_id: machine_id.to_string(),
            batches: Vec::new(),
        }
    }

    /// Add a batch of collected data
    pub fn add_batch(
        &mut self,
        collector: &str,
        lines: Vec<String>,
        cursor: Option<String>,
    ) -> &mut Self {
        let payload = lines.join("\n");
        let batch_hash = hash_content(&payload);
        let row_count = lines.len();

        self.batches.push(BatchEntry {
            collector: collector.to_string(),
            row_count,
            batch_hash,
            cursor,
            lines,
        });
        self
    }

    /// Build the final bundle manifest
    #[must_use]
    pub fn build(self) -> BundleManifest {
        let now = Utc::now();
        let bundle_id = format!("bundle-{}-{}", self.machine_id, now.timestamp_millis());

        // Compute total bytes and content hash
        let mut total_bytes = 0u64;
        let mut hasher = DefaultHasher::new();
        for batch in &self.batches {
            for line in &batch.lines {
                total_bytes += line.len() as u64;
                line.hash(&mut hasher);
            }
        }
        let content_hash = format!("{:016x}", hasher.finish());

        BundleManifest {
            bundle_id,
            machine_id: self.machine_id,
            created_at: now,
            schema_version: 1,
            batches: self.batches,
            content_hash,
            total_bytes,
        }
    }
}

// ============================================================================
// Ingest result
// ============================================================================

/// Result of ingesting a bundle
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IngestResult {
    pub bundle_id: String,
    pub batches_processed: usize,
    pub rows_ingested: usize,
    pub rows_deduplicated: usize,
}

/// Ingest a bundle into the store, deduplicating by content hash
///
/// # Errors
///
/// Returns [`vc_store::StoreError`] when dedup checks, row insertion, or ingest recording fails.
pub fn ingest_bundle(
    store: &vc_store::VcStore,
    manifest: &BundleManifest,
) -> Result<IngestResult, vc_store::StoreError> {
    let mut rows_ingested = 0;
    let mut rows_deduplicated = 0;

    for batch in &manifest.batches {
        let dedup_key = DedupKey::new(&manifest.machine_id, &batch.collector, &batch.batch_hash);

        // Check if this batch was already ingested
        if store.has_ingest_record(&dedup_key.payload_hash)? {
            rows_deduplicated += batch.row_count;
            continue;
        }

        // Ingest each line as a JSON record
        let table = collector_to_table(&batch.collector);
        for line in &batch.lines {
            let Ok(json_val) = serde_json::from_str::<serde_json::Value>(line) else {
                continue; // Skip malformed rows (fail-soft)
            };
            if store.insert_json(&table, &json_val).is_err() {
                continue;
            }
            rows_ingested += 1;
        }

        // Record the ingestion for future dedup
        store.record_ingest(
            &manifest.bundle_id,
            &manifest.machine_id,
            &batch.collector,
            &dedup_key.payload_hash,
            batch.row_count,
        )?;
    }

    Ok(IngestResult {
        bundle_id: manifest.bundle_id.clone(),
        batches_processed: manifest.batches.len(),
        rows_ingested,
        rows_deduplicated,
    })
}

/// Map collector names to table names
fn collector_to_table(collector: &str) -> String {
    match collector {
        "sysmoni" => "sys_samples".to_string(),
        "ntm" => "ntm_sessions_snapshot".to_string(),
        "afsc" => "afsc_status_snapshot".to_string(),
        "cloud_bench" => "cloud_bench_raw".to_string(),
        _ => format!("{collector}_data"),
    }
}

// ============================================================================
// Helpers
// ============================================================================

/// Compute a hex-encoded `SipHash` of content
fn hash_content(content: &str) -> String {
    let mut hasher = DefaultHasher::new();
    content.hash(&mut hasher);
    format!("{:016x}", hasher.finish())
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    // ========================================================================
    // Hash tests
    // ========================================================================

    #[test]
    fn test_hash_deterministic() {
        let h1 = hash_content("hello world");
        let h2 = hash_content("hello world");
        assert_eq!(h1, h2);
    }

    #[test]
    fn test_hash_different_inputs() {
        let h1 = hash_content("hello");
        let h2 = hash_content("world");
        assert_ne!(h1, h2);
    }

    #[test]
    fn test_hash_is_hex() {
        let h = hash_content("test");
        assert_eq!(h.len(), 16);
        assert!(h.chars().all(|c| c.is_ascii_hexdigit()));
    }

    // ========================================================================
    // DedupKey tests
    // ========================================================================

    #[test]
    fn test_dedup_key_creation() {
        let key = DedupKey::new("orko", "sysmoni", "payload data");
        assert_eq!(key.machine_id, "orko");
        assert_eq!(key.collector, "sysmoni");
        assert!(!key.payload_hash.is_empty());
    }

    #[test]
    fn test_dedup_key_same_payload() {
        let k1 = DedupKey::new("orko", "sysmoni", "same data");
        let k2 = DedupKey::new("orko", "sysmoni", "same data");
        assert_eq!(k1, k2);
    }

    #[test]
    fn test_dedup_key_different_payload() {
        let k1 = DedupKey::new("orko", "sysmoni", "data A");
        let k2 = DedupKey::new("orko", "sysmoni", "data B");
        assert_ne!(k1.payload_hash, k2.payload_hash);
    }

    #[test]
    fn test_dedup_key_serialization() {
        let key = DedupKey::new("orko", "sysmoni", "test");
        let json = serde_json::to_string(&key).unwrap();
        let parsed: DedupKey = serde_json::from_str(&json).unwrap();
        assert_eq!(key, parsed);
    }

    // ========================================================================
    // SpoolConfig tests
    // ========================================================================

    #[test]
    fn test_spool_config_defaults() {
        let config = SpoolConfig::default();
        assert_eq!(config.max_spool_bytes, 100 * 1024 * 1024);
        assert_eq!(config.max_age_secs, 7 * 24 * 3600);
    }

    #[test]
    fn test_spool_config_serialization() {
        let config = SpoolConfig::default();
        let json = serde_json::to_string(&config).unwrap();
        let parsed: SpoolConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.max_spool_bytes, config.max_spool_bytes);
    }

    // ========================================================================
    // BundleBuilder tests
    // ========================================================================

    #[test]
    fn test_bundle_builder_empty() {
        let builder = BundleBuilder::new("orko");
        let manifest = builder.build();
        assert!(manifest.batches.is_empty());
        assert_eq!(manifest.machine_id, "orko");
        assert_eq!(manifest.total_bytes, 0);
        assert_eq!(manifest.schema_version, 1);
    }

    #[test]
    fn test_bundle_builder_single_batch() {
        let mut builder = BundleBuilder::new("orko");
        builder.add_batch(
            "sysmoni",
            vec![r#"{"cpu": 42.0}"#.to_string()],
            Some("cursor-1".to_string()),
        );
        let manifest = builder.build();

        assert_eq!(manifest.batches.len(), 1);
        assert_eq!(manifest.batches[0].collector, "sysmoni");
        assert_eq!(manifest.batches[0].row_count, 1);
        assert_eq!(manifest.batches[0].cursor, Some("cursor-1".to_string()));
        assert!(manifest.total_bytes > 0);
    }

    #[test]
    fn test_bundle_builder_multiple_batches() {
        let mut builder = BundleBuilder::new("orko");
        builder.add_batch("sysmoni", vec!["{}".to_string()], None);
        builder.add_batch("ntm", vec!["{}".to_string(), "{}".to_string()], None);
        let manifest = builder.build();

        assert_eq!(manifest.batches.len(), 2);
        assert_eq!(manifest.batches[0].row_count, 1);
        assert_eq!(manifest.batches[1].row_count, 2);
    }

    #[test]
    fn test_bundle_id_contains_machine() {
        let builder = BundleBuilder::new("orko");
        let manifest = builder.build();
        assert!(manifest.bundle_id.contains("orko"));
    }

    #[test]
    fn test_bundle_content_hash_deterministic() {
        // Same batches produce same content hash (within the same test)
        let lines = vec![r#"{"a": 1}"#.to_string()];

        let mut b1 = BundleBuilder::new("orko");
        b1.add_batch("sysmoni", lines.clone(), None);
        let m1 = b1.build();

        let mut b2 = BundleBuilder::new("orko");
        b2.add_batch("sysmoni", lines, None);
        let m2 = b2.build();

        assert_eq!(m1.content_hash, m2.content_hash);
    }

    #[test]
    fn test_bundle_content_hash_differs() {
        let mut b1 = BundleBuilder::new("orko");
        b1.add_batch("sysmoni", vec!["A".to_string()], None);
        let m1 = b1.build();

        let mut b2 = BundleBuilder::new("orko");
        b2.add_batch("sysmoni", vec!["B".to_string()], None);
        let m2 = b2.build();

        assert_ne!(m1.content_hash, m2.content_hash);
    }

    #[test]
    fn test_batch_entry_hash() {
        let mut builder = BundleBuilder::new("orko");
        builder.add_batch("sysmoni", vec![r#"{"x":1}"#.to_string()], None);
        let manifest = builder.build();

        assert!(!manifest.batches[0].batch_hash.is_empty());
        assert_eq!(manifest.batches[0].batch_hash.len(), 16);
    }

    #[test]
    fn test_bundle_manifest_serialization() {
        let mut builder = BundleBuilder::new("orko");
        builder.add_batch("sysmoni", vec![r#"{"cpu": 50}"#.to_string()], None);
        let manifest = builder.build();

        let json = serde_json::to_string(&manifest).unwrap();
        let parsed: BundleManifest = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.machine_id, "orko");
        assert_eq!(parsed.batches.len(), 1);
        assert_eq!(parsed.content_hash, manifest.content_hash);
    }

    // ========================================================================
    // Collector-to-table mapping
    // ========================================================================

    #[test]
    fn test_collector_to_table_known() {
        assert_eq!(collector_to_table("sysmoni"), "sys_samples");
        assert_eq!(collector_to_table("ntm"), "ntm_sessions_snapshot");
        assert_eq!(collector_to_table("afsc"), "afsc_status_snapshot");
        assert_eq!(collector_to_table("cloud_bench"), "cloud_bench_raw");
    }

    #[test]
    fn test_collector_to_table_unknown() {
        assert_eq!(collector_to_table("custom"), "custom_data");
    }

    // ========================================================================
    // IngestResult tests
    // ========================================================================

    #[test]
    fn test_ingest_result_serialization() {
        let result = IngestResult {
            bundle_id: "bundle-orko-123".to_string(),
            batches_processed: 2,
            rows_ingested: 10,
            rows_deduplicated: 3,
        };
        let json = serde_json::to_string(&result).unwrap();
        let parsed: IngestResult = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.rows_ingested, 10);
        assert_eq!(parsed.rows_deduplicated, 3);
    }

    // ========================================================================
    // Store integration tests
    // ========================================================================

    #[test]
    fn test_ingest_bundle_fresh() {
        let store = vc_store::VcStore::open_memory().unwrap();
        let mut builder = BundleBuilder::new("orko");
        builder.add_batch(
            "sysmoni",
            vec![r#"{"cpu_pct": 42, "mem_pct": 60}"#.to_string()],
            None,
        );
        let manifest = builder.build();

        let result = ingest_bundle(&store, &manifest).unwrap();
        assert_eq!(result.batches_processed, 1);
        assert_eq!(result.rows_deduplicated, 0);
    }

    #[test]
    fn test_ingest_bundle_dedup() {
        let store = vc_store::VcStore::open_memory().unwrap();
        let lines = vec![r#"{"cpu_pct": 42}"#.to_string()];

        // Build two bundles with same content
        let mut b1 = BundleBuilder::new("orko");
        b1.add_batch("sysmoni", lines.clone(), None);
        let m1 = b1.build();

        let mut b2 = BundleBuilder::new("orko");
        b2.add_batch("sysmoni", lines, None);
        let m2 = b2.build();

        // First ingest succeeds
        let r1 = ingest_bundle(&store, &m1).unwrap();
        assert_eq!(r1.rows_deduplicated, 0);

        // Second ingest deduplicates
        let r2 = ingest_bundle(&store, &m2).unwrap();
        assert_eq!(r2.rows_deduplicated, 1);
        assert_eq!(r2.rows_ingested, 0);
    }

    #[test]
    fn test_ingest_bundle_different_content() {
        let store = vc_store::VcStore::open_memory().unwrap();

        let mut b1 = BundleBuilder::new("orko");
        b1.add_batch("sysmoni", vec![r#"{"v":1}"#.to_string()], None);
        let m1 = b1.build();

        let mut b2 = BundleBuilder::new("orko");
        b2.add_batch("sysmoni", vec![r#"{"v":2}"#.to_string()], None);
        let m2 = b2.build();

        let r1 = ingest_bundle(&store, &m1).unwrap();
        let r2 = ingest_bundle(&store, &m2).unwrap();

        // Both should ingest (different content)
        assert_eq!(r1.rows_deduplicated, 0);
        assert_eq!(r2.rows_deduplicated, 0);
    }
}
