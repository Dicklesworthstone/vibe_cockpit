//! SSH runner infrastructure with connection pooling
//!
//! Provides async SSH execution for remote machines with connection reuse.

use crate::executor::SshConfig;
use crate::machine::Machine;
use dashmap::DashMap;
use russh::client;
use russh_keys::key::PrivateKeyWithHashAlg;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;
use thiserror::Error;
use tokio::sync::Mutex;
use tracing::{debug, instrument, warn};

/// SSH-specific errors
#[derive(Error, Debug)]
pub enum SshError {
    #[error("Connection failed to {host}: {reason}")]
    ConnectionFailed { host: String, reason: String },

    #[error("Authentication failed for {user}@{host}")]
    AuthFailed { user: String, host: String },

    #[error("Command timed out after {0:?}")]
    Timeout(Duration),

    #[error("Command failed with exit code {code}: {stderr}")]
    CommandFailed { code: u32, stderr: String },

    #[error("No SSH configuration for machine {0}")]
    NoSshConfig(String),

    #[error("Key loading failed: {0}")]
    KeyError(String),

    #[error("Channel error: {0}")]
    ChannelError(String),

    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),

    #[error("Session not connected")]
    NotConnected,
}

/// Configuration for the SSH runner
#[derive(Debug, Clone)]
pub struct SshRunnerConfig {
    /// Connection timeout
    pub connect_timeout: Duration,
    /// Command execution timeout
    pub command_timeout: Duration,
    /// Maximum connection retry attempts
    pub max_retries: u32,
    /// Keepalive interval
    pub keepalive_interval: Duration,
    /// Maximum connections in pool
    pub max_connections: usize,
}

impl Default for SshRunnerConfig {
    fn default() -> Self {
        Self {
            connect_timeout: Duration::from_secs(30),
            command_timeout: Duration::from_secs(60),
            max_retries: 3,
            keepalive_interval: Duration::from_secs(30),
            max_connections: 10,
        }
    }
}

/// Output from command execution
#[derive(Debug, Clone)]
pub struct CommandOutput {
    pub stdout: String,
    pub stderr: String,
    pub exit_code: u32,
}

impl CommandOutput {
    /// Check if command succeeded (exit code 0)
    pub fn success(&self) -> bool {
        self.exit_code == 0
    }
}

/// SSH session wrapper with connection state
struct SshSession {
    handle: client::Handle<SshHandler>,
    config: SshConfig,
    connected_at: std::time::Instant,
}

impl SshSession {
    async fn is_alive(&self) -> bool {
        // Check if the session is still connected
        !self.handle.is_closed()
    }
}

/// SSH client handler for russh
struct SshHandler;

impl client::Handler for SshHandler {
    type Error = SshError;

    async fn check_server_key(
        &mut self,
        _server_public_key: &russh_keys::key::PublicKey,
    ) -> Result<bool, Self::Error> {
        // Accept all server keys (equivalent to StrictHostKeyChecking=accept-new)
        // In production, you'd want to implement proper host key verification
        Ok(true)
    }
}

/// SSH runner with connection pooling
pub struct SshRunner {
    connections: DashMap<String, Arc<Mutex<SshSession>>>,
    config: SshRunnerConfig,
}

impl SshRunner {
    /// Create a new SSH runner with default config
    pub fn new() -> Self {
        Self::with_config(SshRunnerConfig::default())
    }

    /// Create a new SSH runner with custom config
    pub fn with_config(config: SshRunnerConfig) -> Self {
        Self {
            connections: DashMap::new(),
            config,
        }
    }

    /// Execute a command on a remote machine
    #[instrument(skip(self, machine), fields(machine_id = %machine.machine_id))]
    pub async fn exec(&self, machine: &Machine, cmd: &str) -> Result<CommandOutput, SshError> {
        let session = self.get_or_connect(machine).await?;
        let session_guard = session.lock().await;

        self.exec_on_session(&session_guard, cmd).await
    }

    /// Execute command with custom timeout
    pub async fn exec_timeout(
        &self,
        machine: &Machine,
        cmd: &str,
        timeout: Duration,
    ) -> Result<CommandOutput, SshError> {
        match tokio::time::timeout(timeout, self.exec(machine, cmd)).await {
            Ok(result) => result,
            Err(_) => Err(SshError::Timeout(timeout)),
        }
    }

    /// Transfer file from remote to local
    #[instrument(skip(self, machine), fields(machine_id = %machine.machine_id))]
    pub async fn fetch_file(
        &self,
        machine: &Machine,
        remote_path: &str,
        local_path: &Path,
    ) -> Result<(), SshError> {
        // Use cat to read the file content
        let output = self
            .exec(machine, &format!("cat '{}'", escape_path(remote_path)))
            .await?;

        if !output.success() {
            return Err(SshError::CommandFailed {
                code: output.exit_code,
                stderr: output.stderr,
            });
        }

        tokio::fs::write(local_path, output.stdout.as_bytes()).await?;
        Ok(())
    }

    /// Transfer file from local to remote
    #[instrument(skip(self, machine, content), fields(machine_id = %machine.machine_id))]
    pub async fn push_file(
        &self,
        machine: &Machine,
        content: &[u8],
        remote_path: &str,
    ) -> Result<(), SshError> {
        // Use base64 encoding for safe transfer
        let encoded = base64_encode(content);
        let cmd = format!(
            "echo '{}' | base64 -d > '{}'",
            encoded,
            escape_path(remote_path)
        );

        let output = self.exec(machine, &cmd).await?;

        if !output.success() {
            return Err(SshError::CommandFailed {
                code: output.exit_code,
                stderr: output.stderr,
            });
        }

        Ok(())
    }

    /// Check if connection to machine is alive
    #[instrument(skip(self, machine), fields(machine_id = %machine.machine_id))]
    pub async fn ping(&self, machine: &Machine) -> Result<bool, SshError> {
        match self
            .exec_timeout(machine, "echo ping", Duration::from_secs(5))
            .await
        {
            Ok(output) => Ok(output.success() && output.stdout.trim() == "ping"),
            Err(SshError::Timeout(_)) => Ok(false),
            Err(e) => Err(e),
        }
    }

    /// Close all connections in the pool
    pub async fn close_all(&self) {
        self.connections.clear();
    }

    /// Close connection to specific machine
    pub async fn close(&self, machine_id: &str) {
        self.connections.remove(machine_id);
    }

    /// Get pool statistics
    pub fn pool_stats(&self) -> PoolStats {
        PoolStats {
            active_connections: self.connections.len(),
            max_connections: self.config.max_connections,
        }
    }

    /// Get or create a connection to the machine
    async fn get_or_connect(&self, machine: &Machine) -> Result<Arc<Mutex<SshSession>>, SshError> {
        // Check if we have an existing connection
        if let Some(session) = self.connections.get(&machine.machine_id) {
            let session_guard = session.lock().await;
            if session_guard.is_alive().await {
                drop(session_guard);
                return Ok(session.clone());
            }
            // Connection is dead, remove it
            drop(session_guard);
            self.connections.remove(&machine.machine_id);
        }

        // Create new connection
        let ssh_config = machine
            .ssh_config()
            .ok_or_else(|| SshError::NoSshConfig(machine.machine_id.clone()))?;

        let session = self.connect(&ssh_config).await?;
        let session = Arc::new(Mutex::new(session));

        // Respect max connections limit
        if self.connections.len() >= self.config.max_connections {
            // Remove oldest connection (simple LRU would be better)
            if let Some(oldest) = self.connections.iter().next() {
                let key = oldest.key().clone();
                drop(oldest);
                self.connections.remove(&key);
            }
        }

        self.connections
            .insert(machine.machine_id.clone(), session.clone());

        Ok(session)
    }

    /// Establish a new SSH connection
    async fn connect(&self, ssh_config: &SshConfig) -> Result<SshSession, SshError> {
        debug!(
            host = %ssh_config.host,
            user = %ssh_config.user,
            port = ssh_config.port,
            "Connecting to SSH host"
        );

        let config = client::Config {
            inactivity_timeout: Some(self.config.keepalive_interval),
            keepalive_interval: Some(self.config.keepalive_interval),
            keepalive_max: 3,
            ..Default::default()
        };

        let addr = format!("{}:{}", ssh_config.host, ssh_config.port);

        // Connect with timeout
        let handle = match tokio::time::timeout(
            self.config.connect_timeout,
            client::connect(Arc::new(config), &addr, SshHandler),
        )
        .await
        {
            Ok(Ok(handle)) => handle,
            Ok(Err(e)) => {
                return Err(SshError::ConnectionFailed {
                    host: ssh_config.host.clone(),
                    reason: e.to_string(),
                });
            }
            Err(_) => return Err(SshError::Timeout(self.config.connect_timeout)),
        };

        // Authenticate
        let authenticated = if let Some(key_path) = &ssh_config.key_path {
            self.authenticate_with_key(&handle, &ssh_config.user, key_path)
                .await?
        } else {
            // Try agent-based auth
            self.authenticate_with_agent(&handle, &ssh_config.user)
                .await?
        };

        if !authenticated {
            return Err(SshError::AuthFailed {
                user: ssh_config.user.clone(),
                host: ssh_config.host.clone(),
            });
        }

        Ok(SshSession {
            handle,
            config: ssh_config.clone(),
            connected_at: std::time::Instant::now(),
        })
    }

    /// Authenticate using a private key file
    async fn authenticate_with_key(
        &self,
        handle: &client::Handle<SshHandler>,
        user: &str,
        key_path: &str,
    ) -> Result<bool, SshError> {
        let key_path = expand_tilde(key_path);

        let secret_key = russh_keys::load_secret_key(&key_path, None)
            .map_err(|e| SshError::KeyError(format!("Failed to load key {}: {}", key_path, e)))?;

        let key_with_alg = PrivateKeyWithHashAlg::new(Arc::new(secret_key), None).map_err(|e| {
            SshError::KeyError(format!("Failed to create key with hash alg: {}", e))
        })?;

        handle
            .authenticate_publickey(user, key_with_alg)
            .await
            .map_err(|e| SshError::AuthFailed {
                user: user.to_string(),
                host: "unknown".to_string(),
            })
    }

    /// Authenticate using SSH agent
    async fn authenticate_with_agent(
        &self,
        handle: &client::Handle<SshHandler>,
        user: &str,
    ) -> Result<bool, SshError> {
        // Try to connect to SSH agent
        let agent_path = std::env::var("SSH_AUTH_SOCK").ok();

        if let Some(path) = agent_path {
            if let Ok(mut agent) = russh_keys::agent::client::AgentClient::connect_uds(&path).await
            {
                if let Ok(identities) = agent.request_identities().await {
                    for identity in identities {
                        if handle
                            .authenticate_publickey_with(user, Arc::new(identity), &mut agent)
                            .await
                            .is_ok()
                        {
                            return Ok(true);
                        }
                    }
                }
            }
        }

        // Try default key locations
        for key_name in &["id_ed25519", "id_rsa", "id_ecdsa"] {
            let key_path = format!("~/.ssh/{}", key_name);
            if let Ok(true) = self.authenticate_with_key(handle, user, &key_path).await {
                return Ok(true);
            }
        }

        Ok(false)
    }

    /// Execute a command on an existing session
    async fn exec_on_session(
        &self,
        session: &SshSession,
        cmd: &str,
    ) -> Result<CommandOutput, SshError> {
        debug!(cmd = %cmd, "Executing command");

        let mut channel = session
            .handle
            .channel_open_session()
            .await
            .map_err(|e| SshError::ChannelError(e.to_string()))?;

        channel
            .exec(true, cmd)
            .await
            .map_err(|e| SshError::ChannelError(e.to_string()))?;

        let mut stdout = Vec::new();
        let mut stderr = Vec::new();
        let mut exit_code = None;

        loop {
            match tokio::time::timeout(self.config.command_timeout, channel.wait()).await {
                Ok(Some(msg)) => match msg {
                    russh::ChannelMsg::Data { data } => stdout.extend_from_slice(&data),
                    russh::ChannelMsg::ExtendedData { data, ext } => {
                        if ext == 1 {
                            stderr.extend_from_slice(&data);
                        }
                    }
                    russh::ChannelMsg::ExitStatus { exit_status } => {
                        exit_code = Some(exit_status);
                    }
                    russh::ChannelMsg::Eof => break,
                    _ => {}
                },
                Ok(None) => break,
                Err(_) => return Err(SshError::Timeout(self.config.command_timeout)),
            }
        }

        Ok(CommandOutput {
            stdout: String::from_utf8_lossy(&stdout).to_string(),
            stderr: String::from_utf8_lossy(&stderr).to_string(),
            exit_code: exit_code.unwrap_or(0),
        })
    }
}

impl Default for SshRunner {
    fn default() -> Self {
        Self::new()
    }
}

/// Pool statistics
#[derive(Debug, Clone)]
pub struct PoolStats {
    pub active_connections: usize,
    pub max_connections: usize,
}

/// Expand ~ to home directory
fn expand_tilde(path: &str) -> String {
    if path.starts_with("~/") {
        if let Ok(home) = std::env::var("HOME") {
            return path.replacen("~", &home, 1);
        }
    }
    path.to_string()
}

/// Escape path for shell command
fn escape_path(path: &str) -> String {
    path.replace('\'', "'\\''")
}

/// Simple base64 encoding
fn base64_encode(data: &[u8]) -> String {
    use std::io::Write;
    let mut output = Vec::new();
    {
        let mut encoder = Base64Encoder::new(&mut output);
        encoder.write_all(data).unwrap();
        encoder.finish().unwrap();
    }
    String::from_utf8(output).unwrap()
}

/// Simple base64 encoder
struct Base64Encoder<W: std::io::Write> {
    writer: W,
    buffer: [u8; 3],
    buffer_len: usize,
}

impl<W: std::io::Write> Base64Encoder<W> {
    const ALPHABET: &'static [u8; 64] =
        b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

    fn new(writer: W) -> Self {
        Self {
            writer,
            buffer: [0; 3],
            buffer_len: 0,
        }
    }

    fn finish(mut self) -> std::io::Result<()> {
        if self.buffer_len > 0 {
            let mut output = [b'='; 4];
            match self.buffer_len {
                1 => {
                    output[0] = Self::ALPHABET[(self.buffer[0] >> 2) as usize];
                    output[1] = Self::ALPHABET[((self.buffer[0] & 0x03) << 4) as usize];
                }
                2 => {
                    output[0] = Self::ALPHABET[(self.buffer[0] >> 2) as usize];
                    output[1] = Self::ALPHABET
                        [((self.buffer[0] & 0x03) << 4 | self.buffer[1] >> 4) as usize];
                    output[2] = Self::ALPHABET[((self.buffer[1] & 0x0f) << 2) as usize];
                }
                _ => {}
            }
            self.writer.write_all(&output)?;
        }
        Ok(())
    }
}

impl<W: std::io::Write> std::io::Write for Base64Encoder<W> {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        let mut written = 0;
        for &byte in buf {
            self.buffer[self.buffer_len] = byte;
            self.buffer_len += 1;

            if self.buffer_len == 3 {
                let output = [
                    Self::ALPHABET[(self.buffer[0] >> 2) as usize],
                    Self::ALPHABET[((self.buffer[0] & 0x03) << 4 | self.buffer[1] >> 4) as usize],
                    Self::ALPHABET[((self.buffer[1] & 0x0f) << 2 | self.buffer[2] >> 6) as usize],
                    Self::ALPHABET[(self.buffer[2] & 0x3f) as usize],
                ];
                self.writer.write_all(&output)?;
                self.buffer_len = 0;
            }
            written += 1;
        }
        Ok(written)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        self.writer.flush()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ssh_runner_config_default() {
        let config = SshRunnerConfig::default();
        assert_eq!(config.connect_timeout, Duration::from_secs(30));
        assert_eq!(config.command_timeout, Duration::from_secs(60));
        assert_eq!(config.max_retries, 3);
        assert_eq!(config.max_connections, 10);
    }

    #[test]
    fn test_command_output_success() {
        let output = CommandOutput {
            stdout: "hello".to_string(),
            stderr: String::new(),
            exit_code: 0,
        };
        assert!(output.success());

        let failed = CommandOutput {
            stdout: String::new(),
            stderr: "error".to_string(),
            exit_code: 1,
        };
        assert!(!failed.success());
    }

    #[test]
    fn test_ssh_error_display() {
        let err = SshError::ConnectionFailed {
            host: "example.com".to_string(),
            reason: "timeout".to_string(),
        };
        assert!(err.to_string().contains("example.com"));
        assert!(err.to_string().contains("timeout"));

        let auth_err = SshError::AuthFailed {
            user: "ubuntu".to_string(),
            host: "example.com".to_string(),
        };
        assert!(auth_err.to_string().contains("ubuntu@example.com"));
    }

    #[test]
    fn test_expand_tilde() {
        let path = expand_tilde("~/.ssh/id_rsa");
        if std::env::var("HOME").is_ok() {
            assert!(!path.starts_with('~'));
        }

        let abs_path = expand_tilde("/absolute/path");
        assert_eq!(abs_path, "/absolute/path");
    }

    #[test]
    fn test_escape_path() {
        assert_eq!(escape_path("simple"), "simple");
        assert_eq!(escape_path("with'quote"), "with'\\''quote");
    }

    #[test]
    fn test_pool_stats() {
        let runner = SshRunner::new();
        let stats = runner.pool_stats();
        assert_eq!(stats.active_connections, 0);
        assert_eq!(stats.max_connections, 10);
    }

    #[test]
    fn test_base64_encode() {
        assert_eq!(base64_encode(b"hello"), "aGVsbG8=");
        assert_eq!(base64_encode(b"a"), "YQ==");
        assert_eq!(base64_encode(b"ab"), "YWI=");
        assert_eq!(base64_encode(b"abc"), "YWJj");
    }

    #[tokio::test]
    async fn test_ssh_runner_creation() {
        let runner = SshRunner::new();
        assert_eq!(runner.pool_stats().active_connections, 0);
    }

    #[tokio::test]
    async fn test_ssh_runner_with_config() {
        let config = SshRunnerConfig {
            connect_timeout: Duration::from_secs(10),
            command_timeout: Duration::from_secs(30),
            max_retries: 5,
            keepalive_interval: Duration::from_secs(15),
            max_connections: 20,
        };
        let runner = SshRunner::with_config(config);
        assert_eq!(runner.config.max_connections, 20);
    }

    #[tokio::test]
    async fn test_close_all() {
        let runner = SshRunner::new();
        runner.close_all().await;
        assert_eq!(runner.pool_stats().active_connections, 0);
    }
}
