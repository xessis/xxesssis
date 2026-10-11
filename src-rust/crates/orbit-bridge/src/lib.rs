pub use orbit_autocomplete as autocomplete;
pub use orbit_cdp as cdp;
pub use orbit_common as common;
pub use orbit_diff as diff;
pub use orbit_git as git;
pub use orbit_llm as llm;
pub use orbit_mcp as mcp;
pub use orbit_models as models;
pub use orbit_prompts as prompts;
pub use orbit_providers as providers;
pub use orbit_retrieval as retrieval;
pub use orbit_rules as rules;
pub use orbit_runner as runner;
pub use orbit_runtime as runtime;
pub use orbit_shadow as shadow;
pub use orbit_tools as tools;
pub use orbit_worktree as worktree;

pub use orbit_autocomplete::{AutocompleteEngine, FimFormat, FimPrompt};
pub use orbit_cdp::security::{is_cdp_method_allowed, is_navigable_url};
pub use orbit_common::{ChatMode, OrbitError, Result};
pub use orbit_diff::{
    apply_all_blocks, compute_line_diff, parse_search_replace_blocks, DiffOutput,
    SearchReplaceBlock,
};
pub use orbit_git::{GitFileChange, GitScmEngine};
pub use orbit_llm::budget::ContextBudget;
pub use orbit_llm::compaction::{select_compaction_boundary, CompactionBoundaryParams};
pub use orbit_mcp::security::{build_minimal_mcp_env, fingerprint_project_mcp_server};
pub use orbit_models::{get_capabilities, ModelCapabilities, ProviderId};
pub use orbit_prompts::expand_slash_command;
pub use orbit_retrieval::{
    ChunkerConfig, CodeChunk, CodeChunker, EngineStats, HybridConfig, HybridSearchResult,
    IndexingStats, RetrievalEngine,
};
pub use orbit_rules::{
    format_cursor_rules, PromptSectionBuilder, Rule, RuleMatcher, RuleParser, RulesRegistry,
};
pub use orbit_runtime::{CasStore, ThreadHandle};
pub use orbit_shadow::{
    validate_brackets, BracketKind, BracketMismatch, BracketSpan, BracketValidationReport,
    Language, ShadowBuffer, SpeculativeEditSession, SpeculativeFileEdit, TransactionSummary,
};
pub use orbit_tools::file_ops::{atomic_write_file, read_file_lines, FileReadOutput};
pub use orbit_tools::glob::glob_files;
pub use orbit_tools::search::{search_workspace, GrepResult};
pub use orbit_worktree::{
    parse_worktree_list_porcelain, WorktreeInfo, WorktreeManager, WorktreeMetadata,
};
use serde_json::Value;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
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

    // --- Cursor-parity Core Extensions ---

    // Rules engine
    pub fn scan_rules<P: AsRef<Path>>(&self, workspace_root: P) -> Result<RulesRegistry> {
        RulesRegistry::scan_workspace(workspace_root).map_err(Into::into)
    }

    pub fn format_rules(&self, rules: &[&Rule]) -> String {
        format_cursor_rules(rules)
    }

    pub fn match_rules<P: AsRef<Path>>(&self, rules: Vec<Rule>, path: P) -> Result<Vec<Rule>> {
        let matcher = RuleMatcher::new(rules).map_err(OrbitError::from)?;
        let matched = matcher.match_path(path).into_iter().cloned().collect();
        Ok(matched)
    }

    // Code retrieval & chunking
    pub fn create_retrieval_engine(
        &self,
        chunker_config: Option<ChunkerConfig>,
        hybrid_config: Option<HybridConfig>,
    ) -> RetrievalEngine {
        RetrievalEngine::new(
            chunker_config.unwrap_or_default(),
            hybrid_config.unwrap_or_default(),
        )
    }

    pub fn chunk_text(&self, file_path: &str, content: &str) -> Vec<CodeChunk> {
        let chunker = CodeChunker::new(ChunkerConfig::default());
        chunker.chunk_text(file_path, content)
    }

    // Git worktrees
    pub fn worktree_manager(&self, repo_root: impl Into<PathBuf>) -> WorktreeManager {
        WorktreeManager::new(repo_root)
    }

    pub fn parse_worktrees(&self, raw: &str, repo_root: impl AsRef<Path>) -> Vec<WorktreeInfo> {
        parse_worktree_list_porcelain(raw, repo_root.as_ref())
    }

    pub async fn create_worktree(
        &self,
        repo_root: &Path,
        task_id: &str,
        branch_name: &str,
    ) -> Result<WorktreeInfo> {
        WorktreeManager::create_worktree(repo_root, task_id, branch_name)
            .await
            .map_err(Into::into)
    }

    pub async fn list_worktrees(&self, repo_root: &Path) -> Result<Vec<WorktreeInfo>> {
        WorktreeManager::list_worktrees(repo_root)
            .await
            .map_err(Into::into)
    }

    pub async fn remove_worktree(
        &self,
        repo_root: &Path,
        task_id: &str,
        force: bool,
    ) -> Result<()> {
        WorktreeManager::remove_worktree(repo_root, task_id, force)
            .await
            .map_err(Into::into)
    }

    // Speculative shadow buffer & bracket validation
    pub fn validate_brackets(&self, content: &str, path: Option<&str>) -> BracketValidationReport {
        validate_brackets(content, path)
    }

    pub fn create_shadow_session(&self, session_id: impl Into<String>) -> SpeculativeEditSession {
        SpeculativeEditSession::new(session_id)
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
            "format_rules" => {
                let rules: Vec<Rule> = serde_json::from_value(params["rules"].clone())
                    .map_err(OrbitError::Serialization)?;
                let refs: Vec<&Rule> = rules.iter().collect();
                let formatted = self.format_rules(&refs);
                Ok(Value::String(formatted))
            }
            "validate_brackets" => {
                let content = params["content"]
                    .as_str()
                    .or_else(|| params["code"].as_str())
                    .unwrap_or_default();
                let path = params["path"].as_str();
                let report = self.validate_brackets(content, path);
                serde_json::to_value(report).map_err(OrbitError::Serialization)
            }
            "parse_worktrees" => {
                let raw = params["raw"].as_str().unwrap_or_default();
                let root = params["repo_root"].as_str().unwrap_or(".");
                let wts = self.parse_worktrees(raw, Path::new(root));
                serde_json::to_value(wts).map_err(OrbitError::Serialization)
            }
            "chunk_text" => {
                let path = params["path"].as_str().unwrap_or_default();
                let content = params["content"].as_str().unwrap_or_default();
                let chunks = self.chunk_text(path, content);
                serde_json::to_value(chunks).map_err(OrbitError::Serialization)
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

    #[test]
    fn test_bridge_rules_formatting_and_matching() {
        let engine = OrbitCoreEngine::new();
        let rule = Rule::new(
            "test-rule",
            "description",
            vec!["*.rs".to_string()],
            false,
            "Always test your code.",
        );
        let formatted = engine.format_rules(&[&rule]);
        assert!(formatted.contains("<cursor_rules>"));
        assert!(formatted.contains("Always test your code."));

        let matched = engine.match_rules(vec![rule], "src/main.rs").unwrap();
        assert_eq!(matched.len(), 1);
        assert_eq!(matched[0].name, "test-rule");
    }

    #[test]
    fn test_bridge_retrieval_chunk_text() {
        let engine = OrbitCoreEngine::new();
        let code = "fn main() {\n    println!(\"hello\");\n}\n";
        let chunks = engine.chunk_text("main.rs", code);
        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0].file_path, "main.rs");
    }

    #[test]
    fn test_bridge_worktree_porcelain_parse() {
        let engine = OrbitCoreEngine::new();
        let raw = "worktree C:/repo\nHEAD 1111\nbranch refs/heads/main\n\nworktree C:/repo/.orbit/worktrees/task-bridge\nHEAD 2222\nbranch refs/heads/orbit/task-bridge\n\n";
        let wts = engine.parse_worktrees(raw, "C:/repo");
        assert_eq!(wts.len(), 1);
        assert_eq!(wts[0].task_id, "task-bridge");
    }

    #[test]
    fn test_bridge_shadow_bracket_validation() {
        let engine = OrbitCoreEngine::new();
        let valid_code = "fn test() { let x = (1 + 2); }";
        let report = engine.validate_brackets(valid_code, Some("main.rs"));
        assert!(report.is_balanced);
        assert!(report.mismatches.is_empty());

        let invalid_code = "fn test() { let x = (1 + 2; }";
        let report_bad = engine.validate_brackets(invalid_code, Some("main.rs"));
        assert!(!report_bad.is_balanced);
    }

    #[test]
    fn test_bridge_dispatch_new_features() {
        let engine = OrbitCoreEngine::new();

        // format_rules
        let rule_json = serde_json::json!({
            "rules": [{
                "name": "r1",
                "description": "desc",
                "globs": ["*.ts"],
                "always_apply": true,
                "content": "rule content"
            }]
        });
        let res = engine.dispatch("format_rules", rule_json).unwrap();
        assert!(res.as_str().unwrap().contains("rule content"));

        // validate_brackets
        let bracket_json = serde_json::json!({
            "code": "function f() { return [1, 2]; }",
            "path": "app.ts"
        });
        let res = engine.dispatch("validate_brackets", bracket_json).unwrap();
        assert_eq!(res["is_balanced"], true);

        // chunk_text
        let chunk_json = serde_json::json!({
            "path": "lib.rs",
            "content": "pub fn hello() {}"
        });
        let res = engine.dispatch("chunk_text", chunk_json).unwrap();
        assert!(!res.as_array().unwrap().is_empty());
    }
}
