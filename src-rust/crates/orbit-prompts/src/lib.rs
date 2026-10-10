use orbit_common::ChatMode;
use std::fmt::Write;

pub struct EnvironmentContext<'a> {
    pub os: &'a str,
    pub workspace_folders: &'a [&'a str],
    pub active_file: Option<&'a str>,
    pub open_files: &'a [&'a str],
}

pub struct SystemPromptOptions<'a> {
    pub mode: ChatMode,
    pub model_name: &'a str,
    pub env: EnvironmentContext<'a>,
    pub skills: Option<&'a [(&'a str, &'a str)]>, // (name, description)
    pub custom_instructions: Option<&'a str>,
}

pub fn generate_system_prompt(opts: &SystemPromptOptions) -> String {
    let mut out = String::with_capacity(8192);

    let _ = writeln!(
        out,
        "You are an AI coding assistant, powered by {}.\nYou operate in Orbit Editor.",
        opts.model_name
    );

    // 1. Tone and style
    out.push_str("\n<tone_and_style>\n");
    out.push_str("- Be direct, concise, and technically accurate.\n");
    out.push_str("- Do not use emojis unless explicitly requested.\n");
    out.push_str("- Always cite files and line numbers in the format `path:line`.\n");
    out.push_str("</tone_and_style>\n");

    // 2. Skills
    if let Some(skills) = opts.skills {
        if !skills.is_empty() {
            out.push_str("\n<available_skills>\n");
            for (name, desc) in skills {
                let _ = writeln!(out, "  - {}: {}", name, desc);
            }
            out.push_str("</available_skills>\n");
        }
    }

    // 3. Mode policies
    match opts.mode {
        ChatMode::Normal => {
            out.push_str("\n<chat_mode>\n");
            out.push_str("You are in Ask / Normal mode. Provide answers and guidance. Mutating file tools are restricted.\n");
            out.push_str("</chat_mode>\n");
        }
        ChatMode::Plan => {
            out.push_str("\n<plan_mode_guardrails>\n");
            out.push_str("You are in Plan mode. Research and outline the architecture. Only edit markdown plan files.\n");
            out.push_str("</plan_mode_guardrails>\n");
        }
        ChatMode::Agent => {
            out.push_str("\n<agent_mode_guidelines>\n");
            out.push_str("You are in Agent mode. You have full tool execution capabilities.\n");
            out.push_str("- Always inspect files with `Read` before applying edits.\n");
            out.push_str("- Use Fast Apply search/replace blocks or targeted edits.\n");
            out.push_str("</agent_mode_guidelines>\n");
        }
    }

    // 4. Environment
    out.push_str("\n<environment_information>\n");
    let _ = writeln!(out, "- OS: {}", opts.env.os);
    let _ = writeln!(
        out,
        "- Workspace: {}",
        if opts.env.workspace_folders.is_empty() {
            "None"
        } else {
            opts.env.workspace_folders[0]
        }
    );
    if let Some(act) = opts.env.active_file {
        let _ = writeln!(out, "- Active File: {}", act);
    }
    if !opts.env.open_files.is_empty() {
        let _ = writeln!(out, "- Open Files: {}", opts.env.open_files.join(", "));
    }
    out.push_str("</environment_information>\n");

    // 5. Custom instructions
    if let Some(custom) = opts.custom_instructions {
        if !custom.trim().is_empty() {
            out.push_str("\n<user_instructions>\n");
            out.push_str(custom.trim());
            out.push_str("\n</user_instructions>\n");
        }
    }

    out
}

pub fn expand_slash_command(command: &str, user_query: &str) -> Option<String> {
    match command {
        "/explain" => Some(format!(
            "Please explain the selected code or concept thoroughly:\n{}",
            user_query
        )),
        "/fix" => Some(format!(
            "Please identify bugs, type errors, or issues and propose a fix for:\n{}",
            user_query
        )),
        "/test" => Some(format!(
            "Please write comprehensive unit and edge-case tests for:\n{}",
            user_query
        )),
        "/refactor" => Some(format!(
            "Please refactor this code for cleanliness and performance without changing behavior:\n{}",
            user_query
        )),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_system_prompt_agent_mode() {
        let env = EnvironmentContext {
            os: "windows",
            workspace_folders: &["C:/projects/myapp"],
            active_file: Some("src/main.rs"),
            open_files: &["src/main.rs", "src/lib.rs"],
        };

        let skills = [("rust-expert", "Idiomatic Rust guidance")];

        let prompt = generate_system_prompt(&SystemPromptOptions {
            mode: ChatMode::Agent,
            model_name: "claude-3-7-sonnet",
            env,
            skills: Some(&skills),
            custom_instructions: Some("Strict zero slop rules."),
        });

        assert!(prompt.contains("powered by claude-3-7-sonnet"));
        assert!(prompt.contains("<agent_mode_guidelines>"));
        assert!(prompt.contains("rust-expert: Idiomatic Rust guidance"));
        assert!(prompt.contains("- OS: windows"));
        assert!(prompt.contains("Strict zero slop rules."));
    }

    #[test]
    fn test_expand_slash_command() {
        let expanded = expand_slash_command("/fix", "error on line 42").unwrap();
        assert!(expanded.contains("Please identify bugs"));
        assert!(expanded.contains("error on line 42"));

        assert!(expand_slash_command("/unknown", "foo").is_none());
    }
}
