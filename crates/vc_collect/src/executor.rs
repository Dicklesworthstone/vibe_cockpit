//! Command execution utilities for collectors
//!
//! This module provides the `Executor` abstraction for running commands
//! both locally and remotely via SSH. It also provides file operations
//! and `SQLite` query support.

use crate::CollectError;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::process::Stdio;
use std::time::Duration;
use tokio::process::Command;
use tracing::{debug, instrument, warn};

/// Command executor for running shell commands
#[derive(Debug, Clone)]
pub struct Executor {
    /// SSH configuration for remote execution
    ssh_config: Option<SshConfig>,
}

/// SSH configuration for remote machines
#[derive(Debug, Clone)]
pub struct SshConfig {
    /// Remote hostname or IP
    pub host: String,
    /// SSH username
    pub user: String,
    /// Path to SSH private key (optional)
    pub key_path: Option<String>,
    /// SSH port (default 22)
    pub port: u16,
}

impl SshConfig {
    /// Create SSH config with default port
    #[must_use]
    pub fn new(user: impl Into<String>, host: impl Into<String>) -> Self {
        Self {
            host: host.into(),
            user: user.into(),
            key_path: None,
            port: 22,
        }
    }

    /// Set the SSH key path
    #[must_use]
    pub fn with_key(mut self, path: impl Into<String>) -> Self {
        self.key_path = Some(path.into());
        self
    }

    /// Set the SSH port
    #[must_use]
    pub fn with_port(mut self, port: u16) -> Self {
        self.port = port;
        self
    }

    /// Parse from "user@host:port" format
    #[must_use]
    pub fn parse(s: &str) -> Option<Self> {
        if s == "local" {
            return None;
        }

        let (user_host, port) = if let Some((left, port_str)) = s.rsplit_once(':') {
            (left, port_str.parse().unwrap_or(22))
        } else {
            (s, 22)
        };

        let (user, host) = user_host.split_once('@')?;
        Some(Self {
            user: user.to_string(),
            host: host.to_string(),
            key_path: None,
            port,
        })
    }
}

/// Output from command execution
#[derive(Debug, Clone)]
pub struct CommandOutput {
    pub stdout: String,
    pub stderr: String,
    pub exit_code: i32,
}

impl CommandOutput {
    /// Check if the command succeeded (exit code 0)
    #[must_use]
    pub fn success(&self) -> bool {
        self.exit_code == 0
    }
}

/// File stat information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileStat {
    /// File inode number
    pub inode: u64,
    /// File size in bytes
    pub size: u64,
    /// Last modification time
    pub mtime: DateTime<Utc>,
    /// Whether the file exists
    pub exists: bool,
}

impl Executor {
    /// Create a local executor
    #[must_use]
    pub fn local() -> Self {
        Self { ssh_config: None }
    }

    /// Create a remote executor with SSH config
    #[must_use]
    pub fn remote(config: SshConfig) -> Self {
        Self {
            ssh_config: Some(config),
        }
    }

    /// Check if this is a local executor
    #[must_use]
    pub fn is_local(&self) -> bool {
        self.ssh_config.is_none()
    }

    /// Check if a tool is available
    ///
    /// # Errors
    ///
    /// Returns [`CollectError`] only if command execution fails before producing output.
    #[instrument(skip(self))]
    pub async fn check_tool(&self, tool: &str) -> Result<bool, CollectError> {
        let cmd = format!("command -v {tool}");
        match self.run(&cmd, Duration::from_secs(5)).await {
            Ok(output) => Ok(output.exit_code == 0),
            Err(_) => Ok(false),
        }
    }

    /// Run a command with timeout
    ///
    /// # Errors
    ///
    /// Returns [`CollectError`] when command execution fails or times out.
    #[instrument(skip(self))]
    pub async fn run(&self, cmd: &str, timeout: Duration) -> Result<CommandOutput, CollectError> {
        let output = match &self.ssh_config {
            None => self.run_local(cmd, timeout).await?,
            Some(ssh) => self.run_remote(cmd, timeout, ssh).await?,
        };
        Ok(output)
    }

    /// Run a command with timeout, returning stdout on success
    ///
    /// # Errors
    ///
    /// Returns [`CollectError`] when execution fails, times out, or the command exits non-zero.
    pub async fn run_timeout(&self, cmd: &str, timeout: Duration) -> Result<String, CollectError> {
        let output = self.run(cmd, timeout).await?;
        if output.exit_code != 0 {
            return Err(CollectError::ExecutionError(format!(
                "Command failed with exit code {}: {}",
                output.exit_code, output.stderr
            )));
        }
        Ok(output.stdout)
    }

    /// Read an entire file
    ///
    /// # Errors
    ///
    /// Returns [`CollectError`] when the file cannot be read or is missing.
    #[instrument(skip(self))]
    pub async fn read_file(&self, path: &str, timeout: Duration) -> Result<Vec<u8>, CollectError> {
        let cmd = format!("cat {}", shell_escape(path));
        let output = self.run(&cmd, timeout).await?;
        if output.exit_code != 0 {
            if output.stderr.contains("No such file") {
                return Err(CollectError::FileNotFound(path.to_string()));
            }
            return Err(CollectError::ExecutionError(output.stderr));
        }
        Ok(output.stdout.into_bytes())
    }

    /// Read a file from a byte offset to the end (for JSONL tail)
    ///
    /// # Errors
    ///
    /// Returns [`CollectError`] when the file cannot be read or is missing.
    #[instrument(skip(self))]
    pub async fn read_file_range(
        &self,
        path: &str,
        offset: u64,
        timeout: Duration,
    ) -> Result<Vec<u8>, CollectError> {
        // Use tail with byte offset
        // +1 because tail uses 1-based byte positions
        let cmd = format!("tail -c +{} {}", offset + 1, shell_escape(path));
        let output = self.run(&cmd, timeout).await?;
        if output.exit_code != 0 {
            if output.stderr.contains("No such file") {
                return Err(CollectError::FileNotFound(path.to_string()));
            }
            return Err(CollectError::ExecutionError(output.stderr));
        }
        Ok(output.stdout.into_bytes())
    }

    /// Get file stat information (inode, size, mtime)
    ///
    /// # Errors
    ///
    /// Returns [`CollectError`] when stat output cannot be parsed or execution fails.
    #[instrument(skip(self))]
    pub async fn stat(&self, path: &str, timeout: Duration) -> Result<FileStat, CollectError> {
        // Use stat command with format for inode, size, mtime
        // Linux format: stat -c '%i %s %Y'
        // macOS format: stat -f '%i %z %m'
        let cmd = format!(
            "stat -c '%i %s %Y' {} 2>/dev/null || stat -f '%i %z %m' {} 2>/dev/null",
            shell_escape(path),
            shell_escape(path)
        );
        let output = self.run(&cmd, timeout).await?;

        if output.exit_code != 0 || output.stdout.trim().is_empty() {
            // File doesn't exist or can't be statted
            return Ok(FileStat {
                inode: 0,
                size: 0,
                mtime: Utc::now(),
                exists: false,
            });
        }

        let parts: Vec<&str> = output.stdout.split_whitespace().collect();
        if parts.len() < 3 {
            warn!(output = %output.stdout, "Unexpected stat output format");
            return Err(CollectError::ParseError(
                "Unexpected stat output format".to_string(),
            ));
        }

        let inode: u64 = parts[0]
            .parse()
            .map_err(|e| CollectError::ParseError(format!("Invalid inode: {e}")))?;
        let size: u64 = parts[1]
            .parse()
            .map_err(|e| CollectError::ParseError(format!("Invalid size: {e}")))?;
        let mtime_secs: i64 = parts[2]
            .parse()
            .map_err(|e| CollectError::ParseError(format!("Invalid mtime: {e}")))?;

        let mtime = DateTime::from_timestamp(mtime_secs, 0).unwrap_or_else(|| {
            warn!(
                mtime_secs = mtime_secs,
                "Invalid mtime timestamp, using Unix epoch"
            );
            DateTime::UNIX_EPOCH
        });

        Ok(FileStat {
            inode,
            size,
            mtime,
            exists: true,
        })
    }

    /// Run a `SQLite` query and return results as JSON.
    ///
    /// # Errors
    ///
    /// Returns [`CollectError`] when query execution fails or JSON output cannot be parsed.
    #[instrument(skip(self))]
    pub async fn sqlite_query(
        &self,
        db_path: &str,
        query: &str,
        timeout: Duration,
    ) -> Result<Vec<serde_json::Value>, CollectError> {
        // Use sqlite3 with JSON output mode
        let escaped_query = query.replace('\'', "''");
        let cmd = format!(
            "sqlite3 -json {} '{}'",
            shell_escape(db_path),
            escaped_query
        );

        let output = self.run(&cmd, timeout).await?;

        if output.exit_code != 0 {
            return Err(CollectError::SqliteError(output.stderr));
        }

        // Parse JSON output
        if output.stdout.trim().is_empty() {
            return Ok(vec![]);
        }

        let rows: Vec<serde_json::Value> = serde_json::from_str(&output.stdout)
            .map_err(|e| CollectError::ParseError(format!("Failed to parse SQLite JSON: {e}")))?;

        Ok(rows)
    }

    /// Perform an HTTP GET request (using curl)
    ///
    /// # Errors
    ///
    /// Returns [`CollectError`] when request execution fails or exits non-zero.
    #[instrument(skip(self))]
    pub async fn http_get(&self, url: &str, timeout: Duration) -> Result<String, CollectError> {
        let timeout_secs = timeout.as_secs().max(1);
        let cmd = format!("curl -s --max-time {} {}", timeout_secs, shell_escape(url));

        let output = self.run(&cmd, timeout).await?;

        if output.exit_code != 0 {
            return Err(CollectError::HttpError(format!(
                "curl failed with exit code {}: {}",
                output.exit_code, output.stderr
            )));
        }

        Ok(output.stdout)
    }

    /// Check if a file exists
    ///
    /// # Errors
    ///
    /// Returns [`CollectError`] when file stat cannot be retrieved.
    pub async fn file_exists(&self, path: &str, timeout: Duration) -> Result<bool, CollectError> {
        let stat = self.stat(path, timeout).await?;
        Ok(stat.exists)
    }

    /// Get file size in bytes
    ///
    /// # Errors
    ///
    /// Returns [`CollectError`] when file stat fails or the file does not exist.
    pub async fn file_size(&self, path: &str, timeout: Duration) -> Result<u64, CollectError> {
        let stat = self.stat(path, timeout).await?;
        if !stat.exists {
            return Err(CollectError::FileNotFound(path.to_string()));
        }
        Ok(stat.size)
    }

    async fn run_local(&self, cmd: &str, timeout: Duration) -> Result<CommandOutput, CollectError> {
        debug!(cmd = %cmd, "Running local command");

        let mut child = Command::new("sh")
            .arg("-c")
            .arg(cmd)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true)
            .spawn()
            .map_err(|e| CollectError::ExecutionError(e.to_string()))?;

        let result = tokio::time::timeout(timeout, child.wait_with_output()).await;

        match result {
            Ok(Ok(output)) => Ok(CommandOutput {
                stdout: String::from_utf8_lossy(&output.stdout).to_string(),
                stderr: String::from_utf8_lossy(&output.stderr).to_string(),
                exit_code: output.status.code().unwrap_or(-1),
            }),
            Ok(Err(e)) => Err(CollectError::ExecutionError(e.to_string())),
            Err(_) => Err(CollectError::Timeout(timeout)),
        }
    }

    async fn run_remote(
        &self,
        cmd: &str,
        timeout: Duration,
        ssh: &SshConfig,
    ) -> Result<CommandOutput, CollectError> {
        debug!(cmd = %cmd, host = %ssh.host, "Running remote command");

        let mut ssh_cmd = Command::new("ssh");

        // Add key if specified
        if let Some(key) = &ssh.key_path {
            ssh_cmd.arg("-i").arg(key);
        }

        // Add port if non-default
        if ssh.port != 22 {
            ssh_cmd.arg("-p").arg(ssh.port.to_string());
        }

        // Add common SSH options
        ssh_cmd
            .arg("-o")
            .arg("BatchMode=yes")
            .arg("-o")
            .arg("StrictHostKeyChecking=accept-new")
            .arg("-o")
            .arg(format!("ConnectTimeout={}", timeout.as_secs().max(5)));

        // Add host and command
        ssh_cmd
            .arg(format!("{}@{}", ssh.user, ssh.host))
            .arg(cmd)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);

        let child = ssh_cmd
            .spawn()
            .map_err(|e| CollectError::ExecutionError(e.to_string()))?;

        let result = tokio::time::timeout(timeout, child.wait_with_output()).await;

        match result {
            Ok(Ok(output)) => Ok(CommandOutput {
                stdout: String::from_utf8_lossy(&output.stdout).to_string(),
                stderr: String::from_utf8_lossy(&output.stderr).to_string(),
                exit_code: output.status.code().unwrap_or(-1),
            }),
            Ok(Err(e)) => Err(CollectError::ExecutionError(e.to_string())),
            Err(_) => Err(CollectError::Timeout(timeout)),
        }
    }
}

/// Shell-escape a string for safe use in commands
fn shell_escape(s: &str) -> String {
    // Simple escaping: wrap in single quotes, escape embedded single quotes
    format!("'{}'", s.replace('\'', "'\\''"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    #[tokio::test]
    async fn test_local_executor() {
        let executor = Executor::local();
        assert!(executor.is_local());

        let output = executor
            .run("echo hello", Duration::from_secs(5))
            .await
            .unwrap();
        assert_eq!(output.exit_code, 0);
        assert!(output.success());
        assert_eq!(output.stdout.trim(), "hello");
    }

    #[tokio::test]
    async fn test_check_tool() {
        let executor = Executor::local();
        let has_sh = executor.check_tool("sh").await.unwrap();
        assert!(has_sh);

        let has_nonexistent = executor.check_tool("nonexistent_tool_xyz").await.unwrap();
        assert!(!has_nonexistent);
    }

    #[tokio::test]
    async fn test_run_timeout_success() {
        let executor = Executor::local();
        let stdout = executor
            .run_timeout("echo success", Duration::from_secs(5))
            .await
            .unwrap();
        assert_eq!(stdout.trim(), "success");
    }

    #[tokio::test]
    async fn test_run_timeout_failure() {
        let executor = Executor::local();
        let result = executor.run_timeout("exit 1", Duration::from_secs(5)).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_read_file() {
        let mut temp = NamedTempFile::new().unwrap();
        writeln!(temp, "test content").unwrap();

        let executor = Executor::local();
        let content = executor
            .read_file(temp.path().to_str().unwrap(), Duration::from_secs(5))
            .await
            .unwrap();

        assert_eq!(String::from_utf8_lossy(&content).trim(), "test content");
    }

    #[tokio::test]
    async fn test_read_file_not_found() {
        let executor = Executor::local();
        let result = executor
            .read_file("/nonexistent/path/to/file", Duration::from_secs(5))
            .await;
        assert!(matches!(result, Err(CollectError::FileNotFound(_))));
    }

    #[tokio::test]
    async fn test_read_file_range() {
        let mut temp = NamedTempFile::new().unwrap();
        write!(temp, "0123456789").unwrap();

        let executor = Executor::local();
        let content = executor
            .read_file_range(temp.path().to_str().unwrap(), 5, Duration::from_secs(5))
            .await
            .unwrap();

        assert_eq!(String::from_utf8_lossy(&content), "56789");
    }

    #[tokio::test]
    async fn test_stat_existing_file() {
        let temp = NamedTempFile::new().unwrap();

        let executor = Executor::local();
        let stat = executor
            .stat(temp.path().to_str().unwrap(), Duration::from_secs(5))
            .await
            .unwrap();

        assert!(stat.exists);
        assert!(stat.inode > 0);
    }

    #[tokio::test]
    async fn test_stat_nonexistent_file() {
        let executor = Executor::local();
        let stat = executor
            .stat("/nonexistent/path/to/file", Duration::from_secs(5))
            .await
            .unwrap();

        assert!(!stat.exists);
    }

    #[tokio::test]
    async fn test_file_exists() {
        let temp = NamedTempFile::new().unwrap();
        let executor = Executor::local();

        let exists = executor
            .file_exists(temp.path().to_str().unwrap(), Duration::from_secs(5))
            .await
            .unwrap();
        assert!(exists);

        let not_exists = executor
            .file_exists("/nonexistent/path", Duration::from_secs(5))
            .await
            .unwrap();
        assert!(!not_exists);
    }

    #[tokio::test]
    async fn test_file_size() {
        let mut temp = NamedTempFile::new().unwrap();
        write!(temp, "hello").unwrap();

        let executor = Executor::local();
        let size = executor
            .file_size(temp.path().to_str().unwrap(), Duration::from_secs(5))
            .await
            .unwrap();

        assert_eq!(size, 5);
    }

    #[tokio::test]
    async fn test_shell_escape() {
        // Test basic escaping
        assert_eq!(shell_escape("simple"), "'simple'");
        assert_eq!(shell_escape("with spaces"), "'with spaces'");
        assert_eq!(shell_escape("with'quote"), "'with'\\''quote'");
    }

    #[test]
    fn test_ssh_config_parse() {
        let config = SshConfig::parse("ubuntu@example.com:22").unwrap();
        assert_eq!(config.user, "ubuntu");
        assert_eq!(config.host, "example.com");
        assert_eq!(config.port, 22);

        let config2 = SshConfig::parse("user@host").unwrap();
        assert_eq!(config2.user, "user");
        assert_eq!(config2.host, "host");
        assert_eq!(config2.port, 22);

        // "local" should return None
        assert!(SshConfig::parse("local").is_none());

        // Invalid format should return None
        assert!(SshConfig::parse("noatsign").is_none());
    }

    #[test]
    fn test_ssh_config_builder() {
        let config = SshConfig::new("user", "host")
            .with_key("/path/to/key")
            .with_port(2222);

        assert_eq!(config.user, "user");
        assert_eq!(config.host, "host");
        assert_eq!(config.key_path, Some("/path/to/key".to_string()));
        assert_eq!(config.port, 2222);
    }
}
