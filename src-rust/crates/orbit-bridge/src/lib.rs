use orbit_autocomplete::{AutocompleteEngine, FimFormat, FimPrompt};
use orbit_cdp::security::{is_cdp_method_allowed, is_navigable_url};
use orbit_common::{ChatMode, OrbitError, Result};
use orbit_diff::{
    apply_all_blocks, compute_line_diff, parse_search_replace_blocks, DiffOutput,
    SearchReplaceBlock,
};
use orbit_git::{GitFileChange, GitScmEngine};
use orbit_llm::budget::ContextBudget;
use orbit_llm::compaction::{select_compaction_boundary, CompactionBoundaryParams};
use orbit_mcp::security::{build_minimal_mcp_env, fingerprint_project_mcp_server};
use orbit_models::{get_capabilities, ModelCapabilities, ProviderId};
use orbit_prompts::expand_slash_command;
use orbit_runtime::{CasStore, ThreadHandle};
use orbit_tools::file_ops::{atomic_write_file, read_file_lines, FileReadOutput};
use orbit_tools::glob::glob_files;
use orbit_tools::search::{search_workspace, GrepResult};
use serde_json::Value;
use std::collections::HashMap;
use std::path::PathBuf;
use tokio::task::JoinHandle;

pub struct OrbitCoreEngine {
    pub cas: CasStore,
}

impl Default for OrbitCoreEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl OrbitCoreEngine {
    pub fn new() -> Self {
        Self {
            cas: CasStore::new(),
        }
    }

    pub fn diff_lines(&self, old_text: &str, new_text: &str) -> DiffOutput {
        compute_line_diff(old_text, new_text)
    }

    pub fn parse_blocks(&self, text: &str) -> Vec<SearchReplaceBlock> {
        parse_search_replace_blocks(text)
    }

    pub fn apply_blocks(&self, source: &str, blocks: &[SearchReplaceBlock]) -> Result<String> {
        apply_all_blocks(source, blocks)
    }

    pub fn read_file(&self, path: &str, offset: usize, limit: usize) -> Result<FileReadOutput> {
        read_file_lines(path, offset, limit)
    }

    pub fn write_file(&self, path: &str, content: &str) -> Result<()> {
        atomic_write_file(path, content)
    }

    pub fn search(
        &self,
        root: &str,
        pattern: &str,
        case_sensitive: bool,
        max_results: usize,
    ) -> Result<GrepResult> {
        search_workspace(root, pattern, case_sensitive, max_results)
    }

    pub fn glob(&self, root: &str, max_results: usize) -> Result<Vec<PathBuf>> {
        glob_files(root, max_results)
    }

    pub fn budget(&self, context_window: usize, reserve: Option<usize>) -> ContextBudget {
        ContextBudget::calculate(context_window, reserve)
    }

    pub fn select_boundary<'a>(&self, params: &CompactionBoundaryParams<'a>) -> Option<usize> {
        select_compaction_boundary(params)
    }

    pub fn is_url_safe(&self, url: &str) -> bool {
        is_navigable_url(url)
    }

    pub fn is_cdp_allowed(&self, method: &str) -> bool {
        is_cdp_method_allowed(method)
    }

    pub fn sanitize_mcp_env(
        &self,
        parent: &HashMap<String, String>,
        custom: &HashMap<String, String>,
    ) -> HashMap<String, String> {
        build_minimal_mcp_env(parent, custom)
    }

    pub fn fingerprint_mcp(&self, folder_uri: &str, server_name: &str, entry: &Value) -> String {
        fingerprint_project_mcp_server(folder_uri, server_name, entry)
    }

    pub fn spawn_thread(&self, mode: ChatMode) -> (ThreadHandle, JoinHandle<()>) {
        ThreadHandle::spawn(mode)
    }

    pub fn get_model_caps(&self, provider: ProviderId, model_id: &str) -> ModelCapabilities {
        get_capabilities(provider, model_id)
    }

    pub fn expand_command(&self, command: &str, query: &str) -> Option<String> {
        expand_slash_command(command, query)
    }

    pub fn parse_git_status(&self, raw: &str) -> Vec<GitFileChange> {
        GitScmEngine::parse_porcelain_status(raw)
    }

    pub fn build_fim(&self, format: FimFormat, prefix: &str, suffix: &str) -> FimPrompt {
        let engine = AutocompleteEngine::new();
        engine.build_fim_prompt(format, prefix, suffix)
    }

    /// JSON-RPC command dispatcher for external process or FFI binding
    pub fn dispatch(&self, command: &str, params: Value) -> Result<Value> {
        match command {
            "diff" => {
                let old = params["old"].as_str().unwrap_or_default();
                let new = params["new"].as_str().unwrap_or_default();
                let diff = self.diff_lines(old, new);
                serde_json::to_value(diff).map_err(OrbitError::Serialization)
            }
            "apply" => {
                let source = params["source"].as_str().unwrap_or_default();
                let blocks: Vec<SearchReplaceBlock> =
                    serde_json::from_value(params["blocks"].clone())
                        .map_err(OrbitError::Serialization)?;
                let applied = self.apply_blocks(source, &blocks)?;
                Ok(Value::String(applied))
            }
            "budget" => {
                let window = params["window"].as_u64().unwrap_or(32000) as usize;
                let reserve = params["reserve"].as_u64().map(|v| v as usize);
                let budget = self.budget(window, reserve);
                serde_json::to_value(budget).map_err(OrbitError::Serialization)
            }
            "is_url_safe" => {
                let url = params["url"].as_str().unwrap_or_default();
                Ok(Value::Bool(self.is_url_safe(url)))
            }
            "is_cdp_allowed" => {
                let method = params["method"].as_str().unwrap_or_default();
                Ok(Value::Bool(self.is_cdp_allowed(method)))
            }
            "expand_command" => {
                let cmd = params["command"].as_str().unwrap_or_default();
                let query = params["query"].as_str().unwrap_or_default();
                let expanded = self.expand_command(cmd, query);
                serde_json::to_value(expanded).map_err(OrbitError::Serialization)
            }
            "parse_git_status" => {
                let raw = params["raw"].as_str().unwrap_or_default();
                let changes = self.parse_git_status(raw);
                serde_json::to_value(changes).map_err(OrbitError::Serialization)
            }
            _ => Err(OrbitError::ToolExecution(format!(
                "Unknown command: {}",
                command
            ))),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bridge_engine_diff_and_apply() {
        let engine = OrbitCoreEngine::new();
        let old = "const x = 1;\n";
        let new = "const x = 2;\n";
        let diff = engine.diff_lines(old, new);
        assert_eq!(diff.summary.additions, 1);
        assert_eq!(diff.summary.deletions, 1);

        let blocks = vec![SearchReplaceBlock {
            original: "const x = 1;".to_string(),
            replacement: "const x = 2;".to_string(),
        }];
        let applied = engine.apply_blocks(old, &blocks).expect("apply failed");
        assert_eq!(applied, "const x = 2;\n");
    }

    #[test]
    fn test_bridge_dispatch_json_rpc() {
        let engine = OrbitCoreEngine::new();
        let params = serde_json::json!({
            "old": "line 1\n",
            "new": "line 1 modified\n",
        });

        let res = engine.dispatch("diff", params).expect("dispatch failed");
        assert!(res["summary"]["additions"].as_u64().unwrap() >= 1);

        let safe_url = engine
            .dispatch(
                "is_url_safe",
                serde_json::json!({ "url": "https://example.com" }),
            )
            .unwrap();
        assert_eq!(safe_url, Value::Bool(true));

        let blocked_url = engine
            .dispatch(
                "is_url_safe",
                serde_json::json!({ "url": "http://169.254.169.254/" }),
            )
            .unwrap();
        assert_eq!(blocked_url, Value::Bool(false));
    }
}
