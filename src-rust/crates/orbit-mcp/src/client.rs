use crate::protocol::{CallToolResult, JsonRpcRequest, JsonRpcResponse, McpTool};
use crate::security::add_unique_tool_prefix;
use orbit_common::{OrbitError, Result};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use tokio::sync::RwLock;

pub struct McpServerConfig {
    pub name: String,
    pub command: String,
    pub args: Vec<String>,
    pub env: HashMap<String, String>,
}

pub struct McpClient {
    pub name: String,
    prefix: String,
    tools: Arc<RwLock<Vec<McpTool>>>,
    request_id: AtomicU64,
}

impl McpClient {
    pub fn new(name: impl Into<String>) -> Self {
        let name = name.into();
        let prefix = add_unique_tool_prefix(&name, "");
        Self {
            name,
            prefix,
            tools: Arc::new(RwLock::new(Vec::new())),
            request_id: AtomicU64::new(1),
        }
    }

    pub fn next_id(&self) -> u64 {
        self.request_id.fetch_add(1, Ordering::SeqCst)
    }

    pub async fn register_tools(&self, tools: Vec<McpTool>) {
        let mut list = self.tools.write().await;
        *list = tools;
    }

    pub async fn list_tools(&self) -> Vec<McpTool> {
        let list = self.tools.read().await;
        list.clone()
    }

    pub async fn list_prefixed_tools(&self) -> Vec<McpTool> {
        let list = self.tools.read().await;
        list.iter()
            .map(|t| {
                let mut prefixed = t.clone();
                prefixed.name = format!("{}{}", self.prefix, t.name);
                prefixed
            })
            .collect()
    }

    pub fn create_call_tool_request(
        &self,
        tool_name: &str,
        arguments: serde_json::Value,
    ) -> JsonRpcRequest {
        let id = self.next_id();
        let stripped_name = tool_name.strip_prefix(&self.prefix).unwrap_or(tool_name);

        let params = serde_json::json!({
            "name": stripped_name,
            "arguments": arguments,
        });

        JsonRpcRequest::new(id, "tools/call", Some(params))
    }

    pub fn parse_call_tool_response(&self, response: JsonRpcResponse) -> Result<CallToolResult> {
        if let Some(err) = response.error {
            return Err(OrbitError::ToolExecution(format!(
                "MCP error {}: {}",
                err.code, err.message
            )));
        }

        if let Some(res) = response.result {
            serde_json::from_value(res).map_err(OrbitError::Serialization)
        } else {
            Err(OrbitError::ToolExecution(
                "MCP response missing result payload".to_string(),
            ))
        }
    }
}
