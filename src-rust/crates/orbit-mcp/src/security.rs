use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, HashMap};

const ALLOWED_ENV_VARS: &[&str] = &[
    "PATH",
    "PATHEXT",
    "SYSTEMROOT",
    "SYSTEMDRIVE",
    "WINDIR",
    "TEMP",
    "TMP",
    "USER",
    "USERNAME",
    "USERPROFILE",
    "HOME",
    "LANG",
    "LC_ALL",
    "HTTP_PROXY",
    "HTTPS_PROXY",
    "NO_PROXY",
    "http_proxy",
    "https_proxy",
    "no_proxy",
];

/// Filters environment variables down to a minimal safe subset
/// to prevent leaking API keys, auth tokens, and session secrets to child MCP processes.
pub fn build_minimal_mcp_env(
    parent_env: &HashMap<String, String>,
    custom_env: &HashMap<String, String>,
) -> HashMap<String, String> {
    let mut safe_env = HashMap::new();

    for (k, v) in parent_env {
        let upper = k.to_uppercase();
        if ALLOWED_ENV_VARS.contains(&upper.as_str()) {
            safe_env.insert(k.clone(), v.clone());
        }
    }

    // User-configured custom env vars take precedence
    for (k, v) in custom_env {
        safe_env.insert(k.clone(), v.clone());
    }

    safe_env
}

/// Computes SHA-256 fingerprint over canonicalized JSON for MCP project config approval.
pub fn fingerprint_project_mcp_server(
    folder_uri: &str,
    server_name: &str,
    entry: &serde_json::Value,
) -> String {
    let mut map = BTreeMap::new();
    map.insert(
        "folderUri",
        serde_json::Value::String(folder_uri.to_string()),
    );
    map.insert(
        "serverName",
        serde_json::Value::String(server_name.to_string()),
    );
    map.insert("entry", entry.clone());

    let canonical_str = serde_json::to_string(&map).unwrap_or_default();
    let mut hasher = Sha256::new();
    hasher.update(canonical_str.as_bytes());
    format!("{:x}", hasher.finalize())
}

/// Generates a deterministic short 6-char prefix for MCP server tools to avoid naming collisions.
pub fn add_unique_tool_prefix(server_name: &str, tool_name: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(server_name.as_bytes());
    let hex = format!("{:x}", hasher.finalize());
    let prefix = &hex[..6];
    format!("{}_{}", prefix, tool_name)
}
