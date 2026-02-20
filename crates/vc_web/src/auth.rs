//! Authentication and authorization middleware for vc_web
//!
//! Supports multiple API tokens with role-based scopes:
//! - `read`: Read-only access to all API endpoints
//! - `operator`: Read + write for operational actions (ack alerts, run collectors)
//! - `admin`: Full access including token management and configuration

use axum::{
    extract::Request,
    http::{HeaderMap, StatusCode},
    middleware::Next,
    response::{IntoResponse, Json, Response},
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

// ============================================================================
// Roles and scopes
// ============================================================================

/// Role with hierarchical permissions
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    /// Read-only access
    Read,
    /// Read + operational actions
    Operator,
    /// Full admin access
    Admin,
}

impl Role {
    pub fn as_str(&self) -> &str {
        match self {
            Role::Read => "read",
            Role::Operator => "operator",
            Role::Admin => "admin",
        }
    }

    /// Check if this role has at least the required level
    pub fn has_permission(&self, required: Role) -> bool {
        *self >= required
    }

    /// Parse from string
    pub fn parse(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "read" => Some(Role::Read),
            "operator" => Some(Role::Operator),
            "admin" => Some(Role::Admin),
            _ => None,
        }
    }
}

// ============================================================================
// Token definition
// ============================================================================

/// An API token with role and optional restrictions
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiToken {
    /// Display name for the token
    pub name: String,
    /// The token value (bearer token)
    pub token: String,
    /// Role assigned to this token
    pub role: Role,
    /// Optional IP allowlist (empty = allow all)
    #[serde(default)]
    pub allowed_ips: Vec<String>,
    /// Whether the token is active
    #[serde(default = "default_true")]
    pub enabled: bool,
}

fn default_true() -> bool {
    true
}

// ============================================================================
// Auth config
// ============================================================================

/// Authentication configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthConfig {
    /// Whether auth is enabled (false = allow all requests)
    #[serde(default)]
    pub enabled: bool,
    /// API tokens
    #[serde(default)]
    pub tokens: Vec<ApiToken>,
    /// Allow unauthenticated access from localhost
    #[serde(default = "default_true")]
    pub local_bypass: bool,
}

impl Default for AuthConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            tokens: Vec::new(),
            local_bypass: true,
        }
    }
}

impl AuthConfig {
    /// Validate a token string and return the matching ApiToken
    pub fn validate_token(&self, token_str: &str) -> Option<&ApiToken> {
        self.tokens
            .iter()
            .find(|t| t.enabled && t.token == token_str)
    }

    /// Check if a request IP should bypass auth
    pub fn is_local_bypass(&self, ip: &str) -> bool {
        self.local_bypass
            && (ip == "127.0.0.1" || ip == "::1" || ip == "localhost")
    }

    /// Check if a token is allowed from the given IP
    pub fn check_ip_allowlist(&self, token: &ApiToken, ip: &str) -> bool {
        token.allowed_ips.is_empty() || token.allowed_ips.iter().any(|a| a == ip)
    }
}

// ============================================================================
// Auth result
// ============================================================================

/// Result of authentication check
#[derive(Debug, Clone, Serialize)]
pub struct AuthResult {
    pub authenticated: bool,
    pub token_name: Option<String>,
    pub role: Option<Role>,
    pub reason: String,
}

impl AuthResult {
    pub fn allowed(name: &str, role: Role) -> Self {
        Self {
            authenticated: true,
            token_name: Some(name.to_string()),
            role: Some(role),
            reason: "token_valid".to_string(),
        }
    }

    pub fn local_bypass() -> Self {
        Self {
            authenticated: true,
            token_name: None,
            role: Some(Role::Admin),
            reason: "local_bypass".to_string(),
        }
    }

    pub fn denied(reason: &str) -> Self {
        Self {
            authenticated: false,
            token_name: None,
            role: None,
            reason: reason.to_string(),
        }
    }
}

// ============================================================================
// Authenticate a request
// ============================================================================

/// Extract bearer token from Authorization header
pub fn extract_bearer_token(headers: &HeaderMap) -> Option<String> {
    headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .map(|t| t.to_string())
}

/// Authenticate a request against the auth config
pub fn authenticate(
    config: &AuthConfig,
    headers: &HeaderMap,
    client_ip: &str,
) -> AuthResult {
    // If auth is disabled, allow everything
    if !config.enabled {
        return AuthResult::local_bypass();
    }

    // Check local bypass
    if config.is_local_bypass(client_ip) {
        return AuthResult::local_bypass();
    }

    // Extract and validate token
    let Some(token_str) = extract_bearer_token(headers) else {
        return AuthResult::denied("missing_token");
    };

    let Some(api_token) = config.validate_token(&token_str) else {
        return AuthResult::denied("invalid_token");
    };

    // Check IP allowlist
    if !config.check_ip_allowlist(api_token, client_ip) {
        return AuthResult::denied("ip_not_allowed");
    }

    AuthResult::allowed(&api_token.name, api_token.role)
}

/// Check if an auth result has sufficient role
pub fn authorize(result: &AuthResult, required: Role) -> bool {
    match result.role {
        Some(role) => result.authenticated && role.has_permission(required),
        None => false,
    }
}

// ============================================================================
// Middleware helpers (for axum integration)
// ============================================================================

/// Auth state to pass through layers
#[derive(Clone)]
pub struct AuthState {
    pub config: Arc<AuthConfig>,
}

/// Create a 401 Unauthorized response
pub fn unauthorized_response(reason: &str) -> Response {
    let body = serde_json::json!({
        "error": "unauthorized",
        "reason": reason,
        "status": 401
    });
    (StatusCode::UNAUTHORIZED, Json(body)).into_response()
}

/// Create a 403 Forbidden response
pub fn forbidden_response(reason: &str) -> Response {
    let body = serde_json::json!({
        "error": "forbidden",
        "reason": reason,
        "status": 403
    });
    (StatusCode::FORBIDDEN, Json(body)).into_response()
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::HeaderValue;

    fn test_config() -> AuthConfig {
        AuthConfig {
            enabled: true,
            tokens: vec![
                ApiToken {
                    name: "read-token".to_string(),
                    token: "tok-read-123".to_string(),
                    role: Role::Read,
                    allowed_ips: vec![],
                    enabled: true,
                },
                ApiToken {
                    name: "operator-token".to_string(),
                    token: "tok-op-456".to_string(),
                    role: Role::Operator,
                    allowed_ips: vec![],
                    enabled: true,
                },
                ApiToken {
                    name: "admin-token".to_string(),
                    token: "tok-admin-789".to_string(),
                    role: Role::Admin,
                    allowed_ips: vec![],
                    enabled: true,
                },
                ApiToken {
                    name: "restricted-token".to_string(),
                    token: "tok-restricted".to_string(),
                    role: Role::Read,
                    allowed_ips: vec!["10.0.0.1".to_string()],
                    enabled: true,
                },
                ApiToken {
                    name: "disabled-token".to_string(),
                    token: "tok-disabled".to_string(),
                    role: Role::Admin,
                    allowed_ips: vec![],
                    enabled: false,
                },
            ],
            local_bypass: true,
        }
    }

    // ========================================================================
    // Role tests
    // ========================================================================

    #[test]
    fn test_role_ordering() {
        assert!(Role::Admin > Role::Operator);
        assert!(Role::Operator > Role::Read);
    }

    #[test]
    fn test_role_permissions() {
        assert!(Role::Admin.has_permission(Role::Read));
        assert!(Role::Admin.has_permission(Role::Operator));
        assert!(Role::Admin.has_permission(Role::Admin));

        assert!(Role::Operator.has_permission(Role::Read));
        assert!(Role::Operator.has_permission(Role::Operator));
        assert!(!Role::Operator.has_permission(Role::Admin));

        assert!(Role::Read.has_permission(Role::Read));
        assert!(!Role::Read.has_permission(Role::Operator));
        assert!(!Role::Read.has_permission(Role::Admin));
    }

    #[test]
    fn test_role_parse() {
        assert_eq!(Role::parse("read"), Some(Role::Read));
        assert_eq!(Role::parse("operator"), Some(Role::Operator));
        assert_eq!(Role::parse("admin"), Some(Role::Admin));
        assert_eq!(Role::parse("Admin"), Some(Role::Admin));
        assert_eq!(Role::parse("unknown"), None);
    }

    #[test]
    fn test_role_as_str() {
        assert_eq!(Role::Read.as_str(), "read");
        assert_eq!(Role::Operator.as_str(), "operator");
        assert_eq!(Role::Admin.as_str(), "admin");
    }

    #[test]
    fn test_role_serialization() {
        let json = serde_json::to_string(&Role::Operator).unwrap();
        assert_eq!(json, r#""operator""#);
        let parsed: Role = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, Role::Operator);
    }

    // ========================================================================
    // Token validation tests
    // ========================================================================

    #[test]
    fn test_validate_valid_token() {
        let config = test_config();
        let token = config.validate_token("tok-read-123");
        assert!(token.is_some());
        assert_eq!(token.unwrap().name, "read-token");
    }

    #[test]
    fn test_validate_invalid_token() {
        let config = test_config();
        assert!(config.validate_token("invalid").is_none());
    }

    #[test]
    fn test_validate_disabled_token() {
        let config = test_config();
        assert!(config.validate_token("tok-disabled").is_none());
    }

    // ========================================================================
    // Local bypass tests
    // ========================================================================

    #[test]
    fn test_local_bypass_127() {
        let config = test_config();
        assert!(config.is_local_bypass("127.0.0.1"));
    }

    #[test]
    fn test_local_bypass_ipv6() {
        let config = test_config();
        assert!(config.is_local_bypass("::1"));
    }

    #[test]
    fn test_local_bypass_disabled() {
        let mut config = test_config();
        config.local_bypass = false;
        assert!(!config.is_local_bypass("127.0.0.1"));
    }

    #[test]
    fn test_remote_ip_no_bypass() {
        let config = test_config();
        assert!(!config.is_local_bypass("10.0.0.1"));
    }

    // ========================================================================
    // IP allowlist tests
    // ========================================================================

    #[test]
    fn test_ip_allowlist_empty_allows_all() {
        let config = test_config();
        let token = config.validate_token("tok-read-123").unwrap();
        assert!(config.check_ip_allowlist(token, "10.0.0.5"));
    }

    #[test]
    fn test_ip_allowlist_match() {
        let config = test_config();
        let token = config.validate_token("tok-restricted").unwrap();
        assert!(config.check_ip_allowlist(token, "10.0.0.1"));
    }

    #[test]
    fn test_ip_allowlist_no_match() {
        let config = test_config();
        let token = config.validate_token("tok-restricted").unwrap();
        assert!(!config.check_ip_allowlist(token, "10.0.0.99"));
    }

    // ========================================================================
    // Bearer token extraction
    // ========================================================================

    #[test]
    fn test_extract_bearer_token() {
        let mut headers = HeaderMap::new();
        headers.insert("authorization", HeaderValue::from_static("Bearer my-token"));
        assert_eq!(extract_bearer_token(&headers), Some("my-token".to_string()));
    }

    #[test]
    fn test_extract_bearer_no_header() {
        let headers = HeaderMap::new();
        assert!(extract_bearer_token(&headers).is_none());
    }

    #[test]
    fn test_extract_bearer_wrong_scheme() {
        let mut headers = HeaderMap::new();
        headers.insert("authorization", HeaderValue::from_static("Basic abc123"));
        assert!(extract_bearer_token(&headers).is_none());
    }

    // ========================================================================
    // Full authenticate flow
    // ========================================================================

    #[test]
    fn test_auth_disabled() {
        let mut config = test_config();
        config.enabled = false;
        let headers = HeaderMap::new();
        let result = authenticate(&config, &headers, "10.0.0.1");
        assert!(result.authenticated);
        assert_eq!(result.reason, "local_bypass");
    }

    #[test]
    fn test_auth_local_bypass() {
        let config = test_config();
        let headers = HeaderMap::new(); // no token needed
        let result = authenticate(&config, &headers, "127.0.0.1");
        assert!(result.authenticated);
        assert_eq!(result.reason, "local_bypass");
    }

    #[test]
    fn test_auth_missing_token() {
        let config = test_config();
        let headers = HeaderMap::new();
        let result = authenticate(&config, &headers, "10.0.0.1");
        assert!(!result.authenticated);
        assert_eq!(result.reason, "missing_token");
    }

    #[test]
    fn test_auth_invalid_token() {
        let config = test_config();
        let mut headers = HeaderMap::new();
        headers.insert("authorization", HeaderValue::from_static("Bearer bad-token"));
        let result = authenticate(&config, &headers, "10.0.0.1");
        assert!(!result.authenticated);
        assert_eq!(result.reason, "invalid_token");
    }

    #[test]
    fn test_auth_valid_token() {
        let config = test_config();
        let mut headers = HeaderMap::new();
        headers.insert(
            "authorization",
            HeaderValue::from_static("Bearer tok-admin-789"),
        );
        let result = authenticate(&config, &headers, "10.0.0.1");
        assert!(result.authenticated);
        assert_eq!(result.token_name, Some("admin-token".to_string()));
        assert_eq!(result.role, Some(Role::Admin));
    }

    #[test]
    fn test_auth_ip_restricted() {
        let config = test_config();
        let mut headers = HeaderMap::new();
        headers.insert(
            "authorization",
            HeaderValue::from_static("Bearer tok-restricted"),
        );
        // From wrong IP
        let result = authenticate(&config, &headers, "10.0.0.99");
        assert!(!result.authenticated);
        assert_eq!(result.reason, "ip_not_allowed");
    }

    #[test]
    fn test_auth_ip_restricted_allowed() {
        let config = test_config();
        let mut headers = HeaderMap::new();
        headers.insert(
            "authorization",
            HeaderValue::from_static("Bearer tok-restricted"),
        );
        let result = authenticate(&config, &headers, "10.0.0.1");
        assert!(result.authenticated);
    }

    // ========================================================================
    // Authorization tests
    // ========================================================================

    #[test]
    fn test_authorize_read_for_read() {
        let result = AuthResult::allowed("test", Role::Read);
        assert!(authorize(&result, Role::Read));
    }

    #[test]
    fn test_authorize_read_denied_operator() {
        let result = AuthResult::allowed("test", Role::Read);
        assert!(!authorize(&result, Role::Operator));
    }

    #[test]
    fn test_authorize_admin_for_all() {
        let result = AuthResult::allowed("test", Role::Admin);
        assert!(authorize(&result, Role::Read));
        assert!(authorize(&result, Role::Operator));
        assert!(authorize(&result, Role::Admin));
    }

    #[test]
    fn test_authorize_denied_not_authenticated() {
        let result = AuthResult::denied("test");
        assert!(!authorize(&result, Role::Read));
    }

    // ========================================================================
    // Config default tests
    // ========================================================================

    #[test]
    fn test_auth_config_default() {
        let config = AuthConfig::default();
        assert!(!config.enabled);
        assert!(config.tokens.is_empty());
        assert!(config.local_bypass);
    }

    #[test]
    fn test_auth_config_serialization() {
        let config = test_config();
        let json = serde_json::to_string(&config).unwrap();
        let parsed: AuthConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.tokens.len(), 5);
        assert!(parsed.enabled);
    }

    // ========================================================================
    // Response tests
    // ========================================================================

    #[test]
    fn test_unauthorized_response() {
        let resp = unauthorized_response("test reason");
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    }

    #[test]
    fn test_forbidden_response() {
        let resp = forbidden_response("test reason");
        assert_eq!(resp.status(), StatusCode::FORBIDDEN);
    }

    // ========================================================================
    // ApiToken tests
    // ========================================================================

    #[test]
    fn test_api_token_serialization() {
        let token = ApiToken {
            name: "test".to_string(),
            token: "tok-abc".to_string(),
            role: Role::Operator,
            allowed_ips: vec!["10.0.0.1".to_string()],
            enabled: true,
        };
        let json = serde_json::to_string(&token).unwrap();
        let parsed: ApiToken = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.name, "test");
        assert_eq!(parsed.role, Role::Operator);
        assert_eq!(parsed.allowed_ips.len(), 1);
    }
}
