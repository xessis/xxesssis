pub mod harness;
pub mod modes;
pub mod slash_commands;
pub mod system_prompt;
pub mod templates;
pub mod tools;
pub mod xml;

// Re-exports
pub use harness::{augment_chat_messages_with_harness_context, format_harness_timestamp};
pub use modes::{
    mode_system_prompt_guardrails, mode_system_reminder, ToolPolicy,
    BACKGROUND_AGENT_SYSTEM_REMINDER, MERMAID_SYNTAX_REMINDER, MULTITASK_MODE_SYSTEM_REMINDER,
    NORMAL_MODE_SYSTEM_REMINDER, PLAN_MODE_SYSTEM_REMINDER,
};
pub use slash_commands::{
    expand_slash_command, list_slash_commands, SlashCommand, SlashCommandInfo,
};
pub use system_prompt::{
    generate_system_prompt, EnvironmentContext, SkillInfo, SystemPromptOptions,
};
pub use templates::{
    ctrl_k_stream_system_prompt, ctrl_k_stream_user_prompt, git_commit_user_prompt,
    rewrite_code_user_prompt, QuickEditFimTags, CREATE_SEARCH_REPLACE_BLOCKS_SYSTEM_MESSAGE,
    DIVIDER, FINAL, GIT_COMMIT_SYSTEM_MESSAGE, ORIGINAL, REWRITE_CODE_SYSTEM_MESSAGE,
    SEARCH_REPLACE_BLOCK_TEMPLATE,
};
pub use tools::{
    ask_question_tool, builtin_tool_catalog, create_plan_tool, edit_tool, format_tool_call_xml,
    format_tools_xml_definitions, glob_tool, grep_tool, read_tool, terminal_tool, todo_tool,
    web_fetch_tool, web_search_tool, write_tool, ToolDefinition, ToolParameter,
};
pub use xml::{
    escape_xml_attr, escape_xml_text, extract_user_query, sanitize_system_reminder,
    strip_context_tags, wrap_tag, STRIPPED_CONTEXT_TAGS,
};

#[cfg(test)]
mod integration_tests {
    use super::*;
    use orbit_common::{ChatMessage, ChatMode};
    use orbit_rules::Rule;

    #[test]
    fn test_full_pipeline_harness_and_system_prompt() {
        // 1. Setup workspace environment
        let env = EnvironmentContext {
            os: "windows",
            workspace_folders: &["C:/projects/orbiteditor"],
            active_file: Some("src/main.rs"),
            open_files: &["src/main.rs"],
            active_shell_sessions: &["1"],
            directory_structure: Some("src/main.rs"),
        };

        let rule = Rule::new(
            "agent-safety",
            "Safety invariants",
            vec![],
            true,
            "Never do unsafe things.",
        );

        let prompt = generate_system_prompt(&SystemPromptOptions {
            mode: ChatMode::Plan,
            model_name: "claude-3-7-sonnet",
            env,
            skills: None,
            mcp_servers: None,
            rules: Some(&[&rule]),
            custom_instructions: Some("Be extra diligent."),
            include_tool_definitions: true,
            custom_tools: None,
            enable_browser_automation: false,
        });

        // Verify XML sections
        assert!(prompt.contains("<system-communication>"));
        assert!(prompt.contains("<tone_and_style>"));
        assert!(prompt.contains("<plan_mode_guardrails>"));
        assert!(prompt.contains("<citing_code>"));
        assert!(prompt.contains("<always_applied_workspace_rules>"));
        assert!(prompt.contains("Safety invariants"));
        assert!(prompt.contains("<user_instructions>"));
        assert!(prompt.contains("<tool_definitions>"));

        // 2. Harness augmentation on user message
        let messages = vec![
            ChatMessage::system("You are Orbit Assistant"),
            ChatMessage::user("Please build feature X"),
        ];

        let augmented = augment_chat_messages_with_harness_context(&messages, ChatMode::Plan);
        let user_msg = &augmented[1];
        assert!(user_msg.content.contains("<system_reminder>"));
        assert!(user_msg.content.contains("Plan mode is active"));
        assert!(user_msg
            .content
            .contains("<user_query>\nPlease build feature X\n</user_query>"));

        // 3. Slash command expansion
        let expanded = expand_slash_command("/explain", "what is this architecture?").unwrap();
        assert!(expanded.contains("Break down the architecture"));

        // 4. XML tag stripping
        let cleaned = strip_context_tags(&user_msg.content);
        assert!(!cleaned.contains("<system_reminder>"));
        assert!(cleaned.contains("Please build feature X"));
    }
}
