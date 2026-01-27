//! Command execution utilities for collectors

use crate::CollectError;
use std::process::Stdio;
use std::time::Duration;
use tokio::process::Command;
use tracing::{debug, instrument};

/// Command executor for running shell commands
#[derive(Debug, Clone)]
pub struct Executor {
    /// SSH configuration for remote execution
    ssh_config: Option<SshConfig>,
}

/// SSH configuration for remote machines
#[derive(Debug, Clone)]
pub struct SshConfig {
    pub host: String,
    pub user: String,
    pub key_path: Option<String>,
}

/// Output from command execution
#[derive(Debug, Clone)]
pub struct CommandOutput {
    pub stdout: String,
    pub stderr: String,
    pub exit_code: i32,
}

impl Executor {
    /// Create a local executor
    pub fn local() -> Self {
        Self { ssh_config: None }
    }

    /// Create a remote executor with SSH config
    pub fn remote(config: SshConfig) -> Self {
        Self {
            ssh_config: Some(config),
        }
    }

    /// Check if a tool is available
    #[instrument(skip(self))]
    pub async fn check_tool(&self, tool: &str) -> Result<bool, CollectError> {
        let cmd = format!("command -v {tool}");
        match self.run(&cmd, Duration::from_secs(5)).await {
            Ok(output) => Ok(output.exit_code == 0),
            Err(_) => Ok(false),
        }
    }

    /// Run a command with timeout
    #[instrument(skip(self))]
    pub async fn run(&self, cmd: &str, timeout: Duration) -> Result<CommandOutput, CollectError> {
        let output = match &self.ssh_config {
            None => self.run_local(cmd, timeout).await?,
            Some(ssh) => self.run_remote(cmd, timeout, ssh).await?,
        };
        Ok(output)
    }

    /// Run a command with timeout, returning stdout on success
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

    async fn run_local(&self, cmd: &str, timeout: Duration) -> Result<CommandOutput, CollectError> {
        debug!(cmd = %cmd, "Running local command");

        let child = Command::new("sh")
            .arg("-c")
            .arg(cmd)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
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
            .stderr(Stdio::piped());

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

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_local_executor() {
        let executor = Executor::local();
        let output = executor
            .run("echo hello", Duration::from_secs(5))
            .await
            .unwrap();
        assert_eq!(output.exit_code, 0);
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
}
