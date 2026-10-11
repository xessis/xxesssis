use orbit_common::ChatMode;
use serde::{Deserialize, Serialize};

pub const MERMAID_SYNTAX_REMINDER: &str = r#"<mermaid_syntax>
When writing mermaid diagrams:
- Do NOT use spaces in node names/IDs. Use camelCase, PascalCase, or underscores instead.
  - Good: `UserService`, `user_service`, `userAuth`
  - Bad: `User Service`, `user auth`
- Do NOT use HTML tags like `<br/>` or `<br>` - they render as literal text or cause syntax errors.
  - Good: `participant FileSyncer as FS_TypeScript`
  - Bad: `participant FileSyncer as FileSyncer<br/>TypeScript`
- When edge labels contain parentheses, brackets, or other special characters, wrap the label in quotes:
  - Good: `A -->|"O(1) lookup"| B`
  - Bad: `A -->|O(1) lookup| B` (parentheses parsed as node syntax)
- Use double quotes for node labels containing special characters (parentheses, commas, colons):
  - Good: `A["Process (main)"]`, `B["Step 1: Init"]`
  - Bad: `A[Process (main)]` (parentheses parsed as shape syntax)
- Avoid reserved keywords as node IDs: `end`, `subgraph`, `graph`, `flowchart`
  - Good: `endNode[End]`, `processEnd[End]`
  - Bad: `end[End]` (conflicts with subgraph syntax)
- For subgraphs, use explicit IDs with labels in brackets: `subgraph id [Label]`
  - Good: `subgraph auth [Authentication Flow]`
  - Bad: `subgraph Authentication Flow` (spaces cause parsing issues)
- Avoid angle brackets and HTML entities in labels - they render as literal text:
  - Good: `Files[Files Vec]` or `Files[FilesTuple]`
  - Bad: `Files["Vec&lt;T&gt;"]`
- Do NOT use explicit colors or styling - the renderer applies theme colors automatically:
  - Bad: `style A fill:#fff`, `classDef myClass fill:white`, `A:::someStyle`
  - These break in dark mode. Let the default theme handle colors.
- Click events are disabled for security - don't use `click` syntax
</mermaid_syntax>"#;

pub const PLAN_MODE_SYSTEM_REMINDER: &str = r#"Plan mode is active. The user indicated that they do not want you to execute yet — you MUST NOT make any edits, run any non-readonly tools (including changing configs or making commits), or otherwise make any changes to the system. This supersedes any other instructions you have received (for example, to make edits). Instead, you should:

1. Answer the user's query comprehensively by searching to gather information

2. If you do not have enough information to create an accurate plan, you MUST ask the user for more information. If any of the user instructions are ambiguous, you MUST ask for clarification.

3. If the user's request is too broad, you MUST ask the user questions that narrow down the scope of the plan. ONLY ask 1-2 critical questions at a time.

4. If there are multiple valid implementations, each changing the plan significantly, you MUST ask the user to clarify which implementation they want you to use.

5. If you have determined that you will need to ask questions, you should ask them IMMEDIATELY at the start of the conversation. Prefer a small pre-read beforehand only if ≤5 files (~20s) will likely answer them.

6. When you're done researching, present your plan by calling the `create_plan` tool, which will prompt the user to confirm the plan. Do NOT make any file changes or run any tools that modify the system state in any way until the user has confirmed the plan.

7. The plan should be concise, specific and actionable. Cite specific file paths and essential snippets of code. When mentioning files, use markdown links with the full file path (for example, `[backend/src/foo.ts](backend/src/foo.ts)`).

8. Keep plans proportional to the request complexity — don't over-engineer simple tasks.

9. Do NOT use emojis in the plan.

10. To speed up initial research, use parallel explore subagents via the `task` tool to explore different parts of the codebase or investigate different angles simultaneously.

11. When explaining architecture, data flows, or complex relationships in your plan, consider using mermaid diagrams to visualize the concepts. Diagrams can make plans clearer and easier to understand.

12. All questions to the user should be asked using the `AskQuestion` tool.

<mermaid_syntax>
When writing mermaid diagrams:
- Do NOT use spaces in node names/IDs. Use camelCase, PascalCase, or underscores instead.
  - Good: `UserService`, `user_service`, `userAuth`
  - Bad: `User Service`, `user auth`
- Do NOT use HTML tags like `<br/>` or `<br>` - they render as literal text or cause syntax errors.
  - Good: `participant FileSyncer as FS_TypeScript`
  - Bad: `participant FileSyncer as FileSyncer<br/>TypeScript`
- When edge labels contain parentheses, brackets, or other special characters, wrap the label in quotes:
  - Good: `A -->|"O(1) lookup"| B`
  - Bad: `A -->|O(1) lookup| B` (parentheses parsed as node syntax)
- Use double quotes for node labels containing special characters (parentheses, commas, colons):
  - Good: `A["Process (main)"]`, `B["Step 1: Init"]`
  - Bad: `A[Process (main)]` (parentheses parsed as shape syntax)
- Avoid reserved keywords as node IDs: `end`, `subgraph`, `graph`, `flowchart`
  - Good: `endNode[End]`, `processEnd[End]`
  - Bad: `end[End]` (conflicts with subgraph syntax)
- For subgraphs, use explicit IDs with labels in brackets: `subgraph id [Label]`
  - Good: `subgraph auth [Authentication Flow]`
  - Bad: `subgraph Authentication Flow` (spaces cause parsing issues)
- Avoid angle brackets and HTML entities in labels - they render as literal text:
  - Good: `Files[Files Vec]` or `Files[FilesTuple]`
  - Bad: `Files["Vec&lt;T&gt;"]`
- Do NOT use explicit colors or styling - the renderer applies theme colors automatically:
  - Bad: `style A fill:#fff`, `classDef myClass fill:white`, `A:::someStyle`
  - These break in dark mode. Let the default theme handle colors.
- Click events are disabled for security - don't use `click` syntax
</mermaid_syntax>"#;

pub const NORMAL_MODE_SYSTEM_REMINDER: &str = r#"Chat mode is active. You are in read-only mode for exploring code and answering questions.

You MUST NOT edit files, run shell commands, or use any tools that modify the system state. Use read and search tools to gather context and provide concrete answers with file paths and code references.

If the user wants you to implement changes, ask them to switch to Agent mode using the mode selector in the chat UI."#;

pub const MULTITASK_MODE_SYSTEM_REMINDER: &str = r#"Multitask orchestrator mode is active. You are coordinating multiple tasks or delegating work across subagents.

Decompose complex goals into independent sub-tasks, spawn specialized explore or worker subagents using the `task` tool, monitor progress across tasks, aggregate findings, and synthesize clear outcomes for the user."#;

pub const BACKGROUND_AGENT_SYSTEM_REMINDER: &str = r#"Background Agent mode is active. You are running asynchronously in headless mode without an interactive user watching each step.

1. Non-interactive execution: Never pause waiting for interactive confirmation or prompt replies. Default to safe, non-interactive flags (`-y`, `--yes`, `--non-interactive`).
2. Self-verification: Always run tests or compiler checks to verify your work before concluding.
3. Accurate reporting: If blocked by an insurmountable obstacle, record the exact blocker honestly in your final output rather than hallucinating success.
4. Clean termination: Ensure all temporary files or processes are cleanly accounted for."#;

pub const COORDINATOR_MODE_SYSTEM_REMINDER: &str = r#"Project Coordinator mode (Cursor Projects parity) is active. You are the single coordinator agent directing the project.

1. You oversee the overall feature/migration without writing code directly.
2. Break down the user's requirements into concrete tasks and delegate to subagents via the task/subagent tool.
3. Keep shared context and synthesize finished work back to the user."#;

/// Mode-specific instructions injected as `<system_reminder>` on the latest user message.
pub fn mode_system_reminder(mode: ChatMode) -> Option<&'static str> {
    match mode {
        ChatMode::Plan => Some(PLAN_MODE_SYSTEM_REMINDER),
        ChatMode::Normal => Some(NORMAL_MODE_SYSTEM_REMINDER),
        ChatMode::Multitask => Some(MULTITASK_MODE_SYSTEM_REMINDER),
        ChatMode::BackgroundAgent => Some(BACKGROUND_AGENT_SYSTEM_REMINDER),
        ChatMode::Coordinator => Some(COORDINATOR_MODE_SYSTEM_REMINDER),
        ChatMode::Agent => None,
    }
}

/// Mode-specific guardrails included directly in the system prompt.
pub fn mode_system_prompt_guardrails(mode: ChatMode) -> Option<&'static str> {
    match mode {
        ChatMode::Plan => Some(
            r#"<plan_mode_guardrails>
- In plan mode, only edit markdown files.
- If the user is refining the plan, stay in plan mode and keep edits in markdown.
- If the user explicitly asks you to build, implement, or write the code now, ask them to switch to Agent mode using the mode selector in the chat UI.
</plan_mode_guardrails>"#,
        ),
        ChatMode::Normal => Some(
            r#"<normal_mode_guardrails>
- You are in read-only Ask mode.
- Do not make file edits or run mutating commands.
- Provide clear answers with file references and code citations.
</normal_mode_guardrails>"#,
        ),
        ChatMode::Agent => Some(
            r#"<agent_mode_guidelines>
- You are in full Agent mode with tool execution enabled.
- Always use the Read tool before editing code files.
- Fix any introduced linter errors immediately.
- Use Fast Apply search/replace blocks or targeted edits.
</agent_mode_guidelines>"#,
        ),
        ChatMode::Multitask => Some(
            r#"<multitask_mode_guidelines>
- You are in Multitask orchestration mode.
- Decompose broad goals into distinct sub-tasks.
- Delegate sub-tasks to subagents via the task tool to maximize parallelism.
- Synthesize all subagent results into a coherent final deliverable.
</multitask_mode_guidelines>"#,
        ),
        ChatMode::BackgroundAgent => Some(
            r#"<background_agent_guidelines>
- You are running in Background Agent mode.
- Execute autonomously without interactive prompts.
- Thoroughly verify all code changes with tests or build tools.
- Produce a clear final report summarizing changes and verification status.
</background_agent_guidelines>"#,
        ),
        ChatMode::Coordinator => Some(
            r#"<coordinator_mode_guidelines>
- You are the Project Coordinator (Cursor Projects parity).
- You oversee work and direct other agents that write the code.
- Because you delegate rather than execute directly, you remain responsive to user direction.
- Do not make direct code modifications yourself; delegate implementation tasks to subagents.
</coordinator_mode_guidelines>"#,
        ),
    }
}

/// Tool execution permissions and capabilities for a specific mode.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolPolicy {
    pub allow_file_edits: bool,
    pub allow_terminal_commands: bool,
    pub allow_subagents: bool,
    pub allow_network: bool,
    pub read_only_only: bool,
}

impl ToolPolicy {
    pub fn for_mode(mode: ChatMode) -> Self {
        match mode {
            ChatMode::Normal => Self {
                allow_file_edits: false,
                allow_terminal_commands: false,
                allow_subagents: false,
                allow_network: true,
                read_only_only: true,
            },
            ChatMode::Plan => Self {
                allow_file_edits: false, // only markdown plan files through plan tools
                allow_terminal_commands: false,
                allow_subagents: true,
                allow_network: true,
                read_only_only: true,
            },
            ChatMode::Coordinator => Self {
                allow_file_edits: false, // Coordinator delegates code writing to subagents
                allow_terminal_commands: false,
                allow_subagents: true,
                allow_network: true,
                read_only_only: true,
            },
            ChatMode::Agent => Self {
                allow_file_edits: true,
                allow_terminal_commands: true,
                allow_subagents: true,
                allow_network: true,
                read_only_only: false,
            },
            ChatMode::Multitask => Self {
                allow_file_edits: true,
                allow_terminal_commands: true,
                allow_subagents: true,
                allow_network: true,
                read_only_only: false,
            },
            ChatMode::BackgroundAgent => Self {
                allow_file_edits: true,
                allow_terminal_commands: true,
                allow_subagents: true,
                allow_network: true,
                read_only_only: false,
            },
        }
    }

    pub fn is_tool_allowed(&self, tool_name: &str, is_read_only: bool) -> bool {
        if self.read_only_only && !is_read_only {
            return false;
        }

        match tool_name {
            "Write" | "StrReplace" | "edit_file" | "write_file" => self.allow_file_edits,
            "Shell" | "AwaitShell" | "run_terminal_command" => self.allow_terminal_commands,
            "task" | "subagent" => self.allow_subagents,
            "web_search" | "web_fetch" | "WebSearch" | "WebFetch" => self.allow_network,
            _ => true,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mode_system_reminders() {
        assert!(mode_system_reminder(ChatMode::Plan).is_some());
        assert!(mode_system_reminder(ChatMode::Plan)
            .unwrap()
            .contains("Plan mode is active"));
        assert!(mode_system_reminder(ChatMode::Normal)
            .unwrap()
            .contains("Chat mode is active"));
        assert!(mode_system_reminder(ChatMode::Multitask)
            .unwrap()
            .contains("Multitask orchestrator"));
        assert!(mode_system_reminder(ChatMode::BackgroundAgent)
            .unwrap()
            .contains("Background Agent mode"));
        assert!(mode_system_reminder(ChatMode::Coordinator)
            .unwrap()
            .contains("Project Coordinator"));
        assert!(mode_system_reminder(ChatMode::Agent).is_none());
    }

    #[test]
    fn test_mode_tool_policies() {
        let coord_policy = ToolPolicy::for_mode(ChatMode::Coordinator);
        assert!(!coord_policy.allow_file_edits);
        assert!(coord_policy.allow_subagents);
        let normal_policy = ToolPolicy::for_mode(ChatMode::Normal);
        assert!(!normal_policy.is_tool_allowed("Write", false));
        assert!(!normal_policy.is_tool_allowed("Shell", false));
        assert!(normal_policy.is_tool_allowed("Read", true));
        assert!(normal_policy.is_tool_allowed("Grep", true));

        let agent_policy = ToolPolicy::for_mode(ChatMode::Agent);
        assert!(agent_policy.is_tool_allowed("Write", false));
        assert!(agent_policy.is_tool_allowed("Shell", false));
        assert!(agent_policy.is_tool_allowed("Read", true));
    }
}
