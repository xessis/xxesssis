pub mod client;
pub mod protocol;
pub mod security;

pub use client::{McpClient, McpServerConfig};
pub use protocol::{
    CallToolResult, JsonRpcError, JsonRpcNotification, JsonRpcRequest, JsonRpcResponse, McpTool,
    ToolContent,
};
pub use security::{add_unique_tool_prefix, build_minimal_mcp_env, fingerprint_project_mcp_server};

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[test]
    fn test_minimal_env_sanitization() {
        let mut parent_env = HashMap::new();
        parent_env.insert("PATH".to_string(), "/usr/bin:/bin".to_string());
        parent_env.insert("USER".to_string(), "orbit-user".to_string());
        parent_env.insert(
            "OPENAI_API_KEY".to_string(),
            "sk-secret-key-123".to_string(),
        );
        parent_env.insert("ANTHROPIC_API_KEY".to_string(), "sk-ant-secret".to_string());

        let mut custom = HashMap::new();
        custom.insert("MY_TOOL_FLAG".to_string(), "1".to_string());

        let safe = build_minimal_mcp_env(&parent_env, &custom);
        assert!(safe.contains_key("PATH"));
        assert!(safe.contains_key("USER"));
        assert!(safe.contains_key("MY_TOOL_FLAG"));
        assert!(
            !safe.contains_key("OPENAI_API_KEY"),
            "Secret API key leaked into MCP env!"
        );
        assert!(
            !safe.contains_key("ANTHROPIC_API_KEY"),
            "Secret Anthropic key leaked into MCP env!"
        );
    }

    #[test]
    fn test_project_fingerprint_deterministic() {
        let entry1 = serde_json::json!({ "command": "npx", "args": ["-y", "mcp-server"] });
        let entry2 = serde_json::json!({ "command": "npx", "args": ["-y", "mcp-server"] });

        let fp1 = fingerprint_project_mcp_server("file:///workspace", "test-server", &entry1);
        let fp2 = fingerprint_project_mcp_server("file:///workspace", "test-server", &entry2);
        assert_eq!(fp1, fp2);
        assert_eq!(fp1.len(), 64);
    }

    #[tokio::test]
    async fn test_tool_call_request_and_response() {
        let client = McpClient::new("filesystem");
        let tool = McpTool {
            name: "read_file".to_string(),
            description: Some("Reads a file".to_string()),
            input_schema: serde_json::json!({ "type": "object" }),
        };

        client.register_tools(vec![tool]).await;
        let prefixed = client.list_prefixed_tools().await;
        assert_eq!(prefixed.len(), 1);
        assert!(prefixed[0].name.ends_with("_read_file"));

        let req = client.create_call_tool_request(
            &prefixed[0].name,
            serde_json::json!({ "path": "/test.txt" }),
        );

        assert_eq!(req.method, "tools/call");
        let params = req.params.unwrap();
        assert_eq!(params["name"], "read_file");

        let resp = JsonRpcResponse::success(
            req.id,
            serde_json::json!({
                "content": [{ "type": "text", "text": "file content here" }]
            }),
        );

        let call_res = client.parse_call_tool_response(resp).expect("parse failed");
        assert_eq!(call_res.content.len(), 1);
        match &call_res.content[0] {
            ToolContent::Text { text } => assert_eq!(text, "file content here"),
            _ => panic!("Expected text content"),
        }
    }
}
