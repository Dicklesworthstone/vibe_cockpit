//! Tool probing system for detecting installed tools on machines
//!
//! This module provides tool detection capabilities to discover which
//! tools are installed on each machine and their versions.

use crate::CollectError;
use crate::executor::Executor;
use crate::machine::{MachineRegistry, ToolInfo};
use regex::Regex;
use std::sync::Arc;
use std::time::Duration;
use tracing::{debug, info, warn};

/// Specification for detecting a tool
#[derive(Debug, Clone)]
pub struct ToolSpec {
    /// Tool name
    pub name: &'static str,
    /// Commands to try for detection (in order)
    pub detect_commands: &'static [&'static str],
    /// Flag to get version output
    pub version_flag: &'static str,
    /// Regex to extract version from output
    pub version_regex: &'static str,
}

/// Known tools and their detection specs
pub const TOOL_SPECS: &[ToolSpec] = &[
    ToolSpec {
        name: "caut",
        detect_commands: &["command -v caut", "which caut"],
        version_flag: "--version",
        version_regex: r"caut[- ]?v?(\d+\.\d+(?:\.\d+)?)",
    },
    ToolSpec {
        name: "ntm",
        detect_commands: &["command -v ntm", "which ntm"],
        version_flag: "--version",
        version_regex: r"ntm[- ]?v?(\d+\.\d+(?:\.\d+)?)",
    },
    ToolSpec {
        name: "rch",
        detect_commands: &["command -v rch", "which rch"],
        version_flag: "--version",
        version_regex: r"rch[- ]?v?(\d+\.\d+(?:\.\d+)?)",
    },
    ToolSpec {
        name: "rano",
        detect_commands: &["command -v rano", "which rano"],
        version_flag: "--version",
        version_regex: r"rano[- ]?v?(\d+\.\d+(?:\.\d+)?)",
    },
    ToolSpec {
        name: "dcg",
        detect_commands: &["command -v dcg", "which dcg"],
        version_flag: "--version",
        version_regex: r"dcg[- ]?v?(\d+\.\d+(?:\.\d+)?)",
    },
    ToolSpec {
        name: "pt",
        detect_commands: &["command -v pt", "which pt"],
        version_flag: "--version",
        version_regex: r"pt[- ]?v?(\d+\.\d+(?:\.\d+)?)",
    },
    ToolSpec {
        name: "claude-code",
        detect_commands: &["command -v claude", "which claude"],
        version_flag: "--version",
        version_regex: r"(?:claude|claude-code)[- ]?v?(\d+\.\d+(?:\.\d+)?)",
    },
    ToolSpec {
        name: "codex",
        detect_commands: &["command -v codex", "which codex"],
        version_flag: "--version",
        version_regex: r"codex[- ]?v?(\d+\.\d+(?:\.\d+)?)",
    },
    ToolSpec {
        name: "gmi",
        detect_commands: &["command -v gmi", "which gmi"],
        version_flag: "--version",
        version_regex: r"gmi[- ]?v?(\d+\.\d+(?:\.\d+)?)",
    },
    ToolSpec {
        name: "br",
        detect_commands: &["command -v br", "which br"],
        version_flag: "--version",
        version_regex: r"br[- ]?v?(\d+\.\d+(?:\.\d+)?)",
    },
    ToolSpec {
        name: "bv",
        detect_commands: &["command -v bv", "which bv"],
        version_flag: "--version",
        version_regex: r"bv[- ]?v?(\d+\.\d+(?:\.\d+)?)",
    },
    ToolSpec {
        name: "cargo",
        detect_commands: &["command -v cargo", "which cargo"],
        version_flag: "--version",
        version_regex: r"cargo[- ]?v?(\d+\.\d+(?:\.\d+)?)",
    },
    ToolSpec {
        name: "rustc",
        detect_commands: &["command -v rustc", "which rustc"],
        version_flag: "--version",
        version_regex: r"rustc[- ]?v?(\d+\.\d+(?:\.\d+)?)",
    },
    ToolSpec {
        name: "node",
        detect_commands: &["command -v node", "which node"],
        version_flag: "--version",
        version_regex: r"v?(\d+\.\d+(?:\.\d+)?)",
    },
    ToolSpec {
        name: "python3",
        detect_commands: &["command -v python3", "which python3"],
        version_flag: "--version",
        version_regex: r"Python[- ]?v?(\d+\.\d+(?:\.\d+)?)",
    },
];

/// Result of probing a machine
#[derive(Debug, Clone)]
pub struct ProbeResult {
    /// Machine ID that was probed
    pub machine_id: String,
    /// Tools that were found
    pub found_tools: Vec<ToolInfo>,
    /// Errors encountered during probing
    pub errors: Vec<(String, String)>,
}

impl ProbeResult {
    /// Check if the probe was successful (found at least one tool)
    pub fn success(&self) -> bool {
        !self.found_tools.is_empty()
    }

    /// Get count of found tools
    pub fn tool_count(&self) -> usize {
        self.found_tools.len()
    }
}

/// Tool prober for detecting installed tools
pub struct ToolProber {
    timeout: Duration,
}

impl Default for ToolProber {
    fn default() -> Self {
        Self::new()
    }
}

impl ToolProber {
    /// Create a new tool prober with default timeout
    pub fn new() -> Self {
        Self {
            timeout: Duration::from_secs(10),
        }
    }

    /// Set the command timeout
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    /// Probe a machine for all known tools
    pub async fn probe_machine(
        &self,
        machine_id: &str,
        executor: &Executor,
        registry: &Arc<MachineRegistry>,
    ) -> ProbeResult {
        let mut found_tools = Vec::new();
        let mut errors = Vec::new();

        info!(machine_id = %machine_id, "Starting tool probe");

        for spec in TOOL_SPECS {
            match self.probe_tool(executor, spec).await {
                Ok(Some(info)) => {
                    debug!(
                        tool = %spec.name,
                        path = %info.tool_path.as_deref().unwrap_or("?"),
                        version = %info.tool_version.as_deref().unwrap_or("?"),
                        "Tool found"
                    );
                    // Record in database
                    if let Err(e) = registry.record_tool(machine_id, info.clone()) {
                        warn!(tool = %spec.name, error = %e, "Failed to record tool");
                        errors.push((spec.name.to_string(), e.to_string()));
                    }
                    found_tools.push(info);
                }
                Ok(None) => {
                    debug!(tool = %spec.name, "Tool not found");
                    // Record as not available
                    let not_found = ToolInfo {
                        tool_name: spec.name.to_string(),
                        tool_path: None,
                        tool_version: None,
                        is_available: false,
                    };
                    let _ = registry.record_tool(machine_id, not_found);
                }
                Err(e) => {
                    warn!(tool = %spec.name, error = %e, "Probe error");
                    errors.push((spec.name.to_string(), e.to_string()));
                }
            }
        }

        info!(
            machine_id = %machine_id,
            found = found_tools.len(),
            errors = errors.len(),
            "Tool probe complete"
        );

        ProbeResult {
            machine_id: machine_id.to_string(),
            found_tools,
            errors,
        }
    }

    /// Probe for a single tool
    async fn probe_tool(
        &self,
        executor: &Executor,
        spec: &ToolSpec,
    ) -> Result<Option<ToolInfo>, CollectError> {
        // Try each detection command
        for cmd in spec.detect_commands {
            let result = executor.run(cmd, self.timeout).await;
            if let Ok(output) = result {
                if output.exit_code == 0 && !output.stdout.trim().is_empty() {
                    let path = output.stdout.trim().to_string();

                    // Get version
                    let version_cmd = format!("{} {}", path, spec.version_flag);
                    let version = match executor.run(&version_cmd, self.timeout).await {
                        Ok(out) if out.exit_code == 0 => {
                            Self::extract_version(&out.stdout, spec.version_regex)
                                .or_else(|| Self::extract_version(&out.stderr, spec.version_regex))
                        }
                        _ => None,
                    };

                    return Ok(Some(ToolInfo {
                        tool_name: spec.name.to_string(),
                        tool_path: Some(path),
                        tool_version: version,
                        is_available: true,
                    }));
                }
            }
        }
        Ok(None)
    }

    /// Extract version from output using regex
    fn extract_version(output: &str, pattern: &str) -> Option<String> {
        let re = Regex::new(pattern).ok()?;
        re.captures(output)
            .and_then(|c| c.get(1))
            .map(|m| m.as_str().to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_version() {
        // Test caut version
        let output = "caut 0.3.2";
        assert_eq!(
            ToolProber::extract_version(output, r"caut[- ]?v?(\d+\.\d+(?:\.\d+)?)"),
            Some("0.3.2".to_string())
        );

        // Test cargo version
        let output = "cargo 1.82.0 (8f40fc59f 2024-08-21)";
        assert_eq!(
            ToolProber::extract_version(output, r"cargo[- ]?v?(\d+\.\d+(?:\.\d+)?)"),
            Some("1.82.0".to_string())
        );

        // Test node version
        let output = "v20.11.1";
        assert_eq!(
            ToolProber::extract_version(output, r"v?(\d+\.\d+(?:\.\d+)?)"),
            Some("20.11.1".to_string())
        );

        // Test Python version
        let output = "Python 3.12.0";
        assert_eq!(
            ToolProber::extract_version(output, r"Python[- ]?v?(\d+\.\d+(?:\.\d+)?)"),
            Some("3.12.0".to_string())
        );

        // Test no match
        let output = "unknown";
        assert_eq!(
            ToolProber::extract_version(output, r"caut[- ]?v?(\d+\.\d+(?:\.\d+)?)"),
            None
        );
    }

    #[test]
    fn test_tool_specs_valid() {
        // Verify all tool specs have valid regex patterns
        for spec in TOOL_SPECS {
            let re = Regex::new(spec.version_regex);
            assert!(
                re.is_ok(),
                "Invalid regex for {}: {}",
                spec.name,
                spec.version_regex
            );
        }
    }

    #[tokio::test]
    async fn test_probe_local_tools() {
        let prober = ToolProber::new();
        let executor = Executor::local();

        // Probe for sh (should exist on any Unix system)
        let spec = ToolSpec {
            name: "sh",
            detect_commands: &["command -v sh"],
            version_flag: "--version",
            version_regex: r"(\d+\.\d+(?:\.\d+)?)",
        };

        let result = prober.probe_tool(&executor, &spec).await;
        assert!(result.is_ok());
        let tool = result.unwrap();
        assert!(tool.is_some());
        let info = tool.unwrap();
        assert_eq!(info.tool_name, "sh");
        assert!(info.is_available);
        assert!(info.tool_path.is_some());
    }

    #[tokio::test]
    async fn test_probe_nonexistent_tool() {
        let prober = ToolProber::new();
        let executor = Executor::local();

        let spec = ToolSpec {
            name: "nonexistent_tool_xyz",
            detect_commands: &["command -v nonexistent_tool_xyz"],
            version_flag: "--version",
            version_regex: r"(\d+\.\d+(?:\.\d+)?)",
        };

        let result = prober.probe_tool(&executor, &spec).await;
        assert!(result.is_ok());
        assert!(result.unwrap().is_none());
    }
}
