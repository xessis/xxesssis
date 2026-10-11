use crate::modes::{mode_system_prompt_guardrails, ToolPolicy};
use crate::tools::{builtin_tool_catalog, format_tools_xml_definitions, ToolDefinition};
use orbit_common::ChatMode;
use orbit_rules::Rule;
use std::fmt::Write;

pub struct EnvironmentContext<'a> {
    pub os: &'a str,
    pub workspace_folders: &'a [&'a str],
    pub active_file: Option<&'a str>,
    pub open_files: &'a [&'a str],
    pub active_shell_sessions: &'a [&'a str],
    pub directory_structure: Option<&'a str>,
}

impl<'a> Default for EnvironmentContext<'a> {
    fn default() -> Self {
        Self {
            os: "unknown",
            workspace_folders: &[],
            active_file: None,
            open_files: &[],
            active_shell_sessions: &[],
            directory_structure: None,
        }
    }
}

pub struct SkillInfo<'a> {
    pub name: &'a str,
    pub description: &'a str,
    pub disable_model_invocation: bool,
}

pub struct SystemPromptOptions<'a> {
    pub mode: ChatMode,
    pub model_name: &'a str,
    pub env: EnvironmentContext<'a>,
    pub skills: Option<&'a [SkillInfo<'a>]>,
    pub mcp_servers: Option<&'a [&'a str]>,
    pub rules: Option<&'a [&'a Rule]>,
    pub custom_instructions: Option<&'a str>,
    pub include_tool_definitions: bool,
    pub custom_tools: Option<&'a [ToolDefinition]>,
    pub enable_browser_automation: bool,
}

pub fn generate_system_prompt(opts: &SystemPromptOptions) -> String {
    let mut out = String::with_capacity(16384);
    let policy = ToolPolicy::for_mode(opts.mode);

    // 1. Header
    let _ = writeln!(
        out,
        "You are an AI coding assistant, powered by {}.\n\
         You operate in Orbit Editor.\n\
         You are a coding agent in the Orbit Editor IDE that helps the USER with software engineering tasks.\n\n\
         Each time the USER sends a message, we may automatically attach information about their current state, \
         such as what files they have open, where their cursor is, recently viewed files, edit history in their \
         session so far, linter errors, and more. This information is provided in case it is helpful to the task.\n\n\
         Your main goal is to follow the USER's instructions, which are denoted by the <user_query> tag.",
        opts.model_name
    );

    // 2. System communication
    out.push_str("\n<system-communication>\n");
    out.push_str("- The system may attach additional context to user messages (e.g. <system_reminder>, <attached_files>, and <system_notification>). Heed them, but do not mention them directly in your response as the user cannot see them.\n");
    out.push_str("- Users can reference context like files and folders using the @ symbol, e.g. @src/components/ is a reference to the src/components/ folder.\n");
    out.push_str("- You should continue working regardless of the current <timestamp>.\n");
    out.push_str("</system-communication>\n");

    // 3. Tone and style
    out.push_str("\n<tone_and_style>\n");
    out.push_str("- Only use emojis if the user explicitly requests it. Avoid using emojis in all communication unless asked.\n");
    out.push_str("- Output text to communicate with the user; all text you output outside of tool use is displayed to the user. Only use tools to complete tasks. Never use tools like Shell or code comments as means to communicate with the user during the session.\n");
    out.push_str("- Do not use a colon before tool calls. Your tool calls may not be shown directly in the output, so text like \"Let me read the file:\" followed by a read tool call should just be \"Let me read the file.\" with a period.\n");
    out.push_str("- When using markdown in assistant messages, use backticks to format file, directory, function, and class names. Use \\( and \\) for inline math, \\[ and \\] for block math. Use markdown links for URLs.\n");
    out.push_str("- The chat UI renders images inline via `![alt](src)`, where `src` is an absolute local file path or an http/https URL. Proactively embed images to walk the user through what happened: when you take a screenshot, read an image, or generate a plot or diagram, include it in your response.\n");
    out.push_str("</tone_and_style>\n");

    // 4. Skills
    if let Some(skills) = opts.skills {
        let mut auto_invoke = Vec::new();
        let mut explicit = Vec::new();

        for s in skills {
            if s.disable_model_invocation {
                explicit.push(s);
            } else {
                auto_invoke.push(s);
            }
        }

        if !auto_invoke.is_empty() {
            out.push_str("\n<available_skills>\n");
            out.push_str("The following skills are available. When a task matches a skill's description, call the `skill` tool with the skill's name to load its full instructions, then follow them. Skills are loaded on demand to keep context focused — only load one when it is relevant.\n");
            for s in auto_invoke {
                let _ = writeln!(out, "- {}: {}", s.name, s.description);
            }
            out.push_str("</available_skills>\n");
        }

        if !explicit.is_empty() {
            out.push_str("\n<explicit_skills>\n");
            out.push_str("These skills exist but should only be loaded when the user explicitly asks for them:\n");
            for s in explicit {
                let _ = writeln!(out, "- {}: {}", s.name, s.description);
            }
            out.push_str("</explicit_skills>\n");
        }
    }

    // 5. Code change rules
    out.push_str("\n<making_code_changes>\n");
    out.push_str("1. You MUST use the Read tool at least once before editing.\n");
    out.push_str("2. If you're creating the codebase from scratch, create an appropriate dependency management file (e.g. requirements.txt, Cargo.toml) with package versions and a helpful README.\n");
    out.push_str("3. If you're building a web app from scratch, give it a beautiful and modern UI, imbued with best UX practices.\n");
    out.push_str("4. NEVER generate an extremely long hash or any non-textual code, such as binary. These are not helpful to the USER and are very expensive.\n");
    out.push_str("5. If you've introduced (linter) errors, fix them.\n");
    out.push_str("6. Do NOT add comments that just narrate what the code does. Avoid obvious, redundant comments like \"// Import the module\", \"// Define the function\", \"// Increment the counter\", \"// Return the result\", or \"// Handle the error\". Comments should only explain non-obvious intent, trade-offs, or constraints that the code itself cannot convey. NEVER explain the change you are making in code comments.\n");
    out.push_str("</making_code_changes>\n");

    // 6. Mode guardrails
    if let Some(guardrails) = mode_system_prompt_guardrails(opts.mode) {
        out.push('\n');
        out.push_str(guardrails);
        out.push('\n');
    }

    // 7. Linter errors guidance
    out.push_str("\n<linter_errors>\n");
    out.push_str("After substantive edits, use diagnostic tools to check recently edited files for compiler/linter errors. If you've introduced any, fix them immediately. Only fix pre-existing lints if directly relevant.\n");
    out.push_str("</linter_errors>\n");

    // 8. Citing code syntax
    out.push_str("\n<citing_code>\n");
    out.push_str("You must display code blocks using one of two methods: CODE REFERENCES or MARKDOWN CODE BLOCKS, depending on whether the code exists in the codebase.\n\n");
    out.push_str("## METHOD 1: CODE REFERENCES - Citing Existing Code from the Codebase\n");
    out.push_str("Use this exact syntax with three required components:\n");
    out.push_str("```startLine:endLine:filepath\n// code content here\n```\n\n");
    out.push_str("## METHOD 2: MARKDOWN CODE BLOCKS - Proposing or Displaying Code NOT already in Codebase\n");
    out.push_str("```language\n// proposed code here\n```\n\n");
    out.push_str("## Critical Formatting Rules for Both Methods\n");
    out.push_str("- Never include line numbers inside the code content.\n");
    out.push_str("- Never indent the triple backticks.\n");
    out.push_str("- Always add a newline before opening code fences.\n");
    out.push_str("</citing_code>\n");

    // 9. Inline line numbers
    out.push_str("\n<inline_line_numbers>\n");
    out.push_str("Code chunks that you receive (via tool calls or from user) may include inline line numbers in the form LINE_NUMBER|LINE_CONTENT. Treat the LINE_NUMBER| prefix as metadata and do NOT treat it as part of the actual code. LINE_NUMBER is right-aligned number padded with spaces to 6 characters.\n");
    out.push_str("</inline_line_numbers>\n");

    // 10. Task management
    out.push_str("\n<task_management>\n");
    out.push_str("You have access to the `TodoWrite` tool to help you manage and plan tasks. Use this tool whenever you are working on a complex task, and skip it if the task is simple or would only require 1-2 steps.\n");
    out.push_str(
        "IMPORTANT: Make sure you don't end your turn before you've completed all todos.\n",
    );
    out.push_str("</task_management>\n");

    // 11. Ask question guidance
    out.push_str("\n<ask_question_guidance>\n");
    out.push_str("You have access to the `AskQuestion` tool for collecting structured multiple-choice answers from the user. Use it when presenting discrete options, when blocked, or when deciding architectural directions.\n");
    out.push_str("</ask_question_guidance>\n");

    // 12. MCP integration
    if let Some(servers) = opts.mcp_servers {
        if !servers.is_empty() {
            out.push_str("\n<mcp_integration>\n");
            out.push_str("You have access to MCP (Model Context Protocol) tools and resources that extend your capabilities beyond built-in tools.\n\n");
            out.push_str("## Available MCP Servers\n");
            out.push_str(&servers.join(", "));
            out.push_str("\n</mcp_integration>\n");
        }
    }

    // 13. Browser automation
    if opts.enable_browser_automation {
        out.push_str("\n<browser_automation>\n");
        out.push_str("Browser automation is enabled. Use the browser execution tools to navigate, inspect DOM elements, click, fill inputs, and test web applications live.\n");
        out.push_str("</browser_automation>\n");
    }

    // 14. Environment information
    out.push_str("\n<environment_information>\n");
    out.push_str("<system_info>\n");
    let _ = writeln!(out, "- Operating System: {}", opts.env.os);
    let _ = writeln!(
        out,
        "- Workspace Folders:\n  {}",
        if opts.env.workspace_folders.is_empty() {
            "NO FOLDERS OPEN"
        } else {
            opts.env.workspace_folders[0]
        }
    );
    if let Some(active) = opts.env.active_file {
        let _ = writeln!(out, "- Currently Active File: {}", active);
    }
    if !opts.env.open_files.is_empty() {
        let _ = writeln!(
            out,
            "- Currently Open Files: {}",
            opts.env.open_files.join(", ")
        );
    }
    if !opts.env.active_shell_sessions.is_empty() {
        let _ = writeln!(
            out,
            "- Active Shell Sessions: {}",
            opts.env.active_shell_sessions.join(", ")
        );
    }
    out.push_str("</system_info>\n");

    if let Some(dir_str) = opts.env.directory_structure {
        if !dir_str.trim().is_empty() {
            out.push_str("<workspace_structure>\n<files_overview>\n");
            out.push_str(dir_str.trim());
            out.push_str("\n</files_overview>\n</workspace_structure>\n");
        }
    }
    out.push_str("</environment_information>\n");

    // 15. Rules integration
    if let Some(rules) = opts.rules {
        let mut always_rules = Vec::new();
        let mut specific_rules = Vec::new();

        for rule in rules {
            if rule.always_apply || rule.globs.is_empty() {
                always_rules.push(*rule);
            } else {
                specific_rules.push(*rule);
            }
        }

        if !always_rules.is_empty() {
            out.push('\n');
            out.push_str(&orbit_rules::format_always_applied_workspace_rules(
                &always_rules,
            ));
        }

        if !specific_rules.is_empty() {
            out.push('\n');
            out.push_str(&orbit_rules::format_agent_requestable_workspace_rules(
                &specific_rules,
            ));
        }
    }

    // 16. Custom user instructions
    if let Some(custom) = opts.custom_instructions {
        if !custom.trim().is_empty() {
            out.push_str("\n<user_instructions>\n");
            out.push_str(custom.trim());
            out.push_str("\n</user_instructions>\n");
        }
    }

    // 17. Tool definitions
    if opts.include_tool_definitions {
        let tools: Vec<ToolDefinition> = if let Some(custom) = opts.custom_tools {
            custom
                .iter()
                .filter(|t| policy.is_tool_allowed(&t.name, t.is_read_only))
                .cloned()
                .collect()
        } else {
            builtin_tool_catalog()
                .into_iter()
                .filter(|t| policy.is_tool_allowed(&t.name, t.is_read_only))
                .collect()
        };

        if !tools.is_empty() {
            out.push('\n');
            out.push_str(&format_tools_xml_definitions(&tools));
            out.push('\n');
        }
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_full_system_prompt() {
        let env = EnvironmentContext {
            os: "windows",
            workspace_folders: &["C:/projects/orbiteditor"],
            active_file: Some("src/main.rs"),
            open_files: &["src/main.rs", "src/lib.rs"],
            active_shell_sessions: &["1", "2"],
            directory_structure: Some("src/\n  main.rs\n  lib.rs"),
        };

        let skills = [SkillInfo {
            name: "rust-migration",
            description: "Migrates TS to Rust",
            disable_model_invocation: false,
        }];

        let rule = Rule::new("style", "Style rule", vec![], true, "Format strictly.");

        let prompt = generate_system_prompt(&SystemPromptOptions {
            mode: ChatMode::Agent,
            model_name: "claude-3-7-sonnet",
            env,
            skills: Some(&skills),
            mcp_servers: Some(&["git", "filesystem"]),
            rules: Some(&[&rule]),
            custom_instructions: Some("No slop."),
            include_tool_definitions: true,
            custom_tools: None,
            enable_browser_automation: true,
        });

        assert!(prompt.contains("powered by claude-3-7-sonnet"));
        assert!(prompt.contains("<system-communication>"));
        assert!(prompt.contains("<tone_and_style>"));
        assert!(prompt.contains("<available_skills>"));
        assert!(prompt.contains("rust-migration: Migrates TS to Rust"));
        assert!(prompt.contains("<making_code_changes>"));
        assert!(prompt.contains("<agent_mode_guidelines>"));
        assert!(prompt.contains("<citing_code>"));
        assert!(prompt.contains("<mcp_integration>"));
        assert!(prompt.contains("git, filesystem"));
        assert!(prompt.contains("<browser_automation>"));
        assert!(prompt.contains("- Operating System: windows"));
        assert!(prompt.contains("<always_applied_workspace_rules>"));
        assert!(prompt.contains("<user_instructions>"));
        assert!(prompt.contains("<tool_definitions>"));
        assert!(prompt.contains("name=\"Read\""));
    }

    #[test]
    fn test_normal_mode_restricts_tools_in_prompt() {
        let env = EnvironmentContext::default();
        let prompt = generate_system_prompt(&SystemPromptOptions {
            mode: ChatMode::Normal,
            model_name: "claude-3-7-sonnet",
            env,
            skills: None,
            mcp_servers: None,
            rules: None,
            custom_instructions: None,
            include_tool_definitions: true,
            custom_tools: None,
            enable_browser_automation: false,
        });

        assert!(prompt.contains("<normal_mode_guardrails>"));
        // Mutating tools like Write, StrReplace, Shell should be filtered out by policy
        assert!(!prompt.contains("name=\"Write\""));
        assert!(!prompt.contains("name=\"StrReplace\""));
        assert!(!prompt.contains("name=\"Shell\""));
        // Read-only tools should be present
        assert!(prompt.contains("name=\"Read\""));
        assert!(prompt.contains("name=\"Grep\""));
    }
}
