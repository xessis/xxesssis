use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum SlashCommand {
    Explain,
    Fix,
    Test,
    Refactor,
    Review,
    Plan,
}

impl std::str::FromStr for SlashCommand {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let name = s.trim().trim_start_matches('/');
        match name.to_ascii_lowercase().as_str() {
            "explain" => Ok(Self::Explain),
            "fix" => Ok(Self::Fix),
            "test" => Ok(Self::Test),
            "refactor" => Ok(Self::Refactor),
            "review" => Ok(Self::Review),
            "plan" => Ok(Self::Plan),
            _ => Err(()),
        }
    }
}

impl SlashCommand {
    pub fn from_command_name(s: &str) -> Option<Self> {
        s.parse().ok()
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Explain => "/explain",
            Self::Fix => "/fix",
            Self::Test => "/test",
            Self::Refactor => "/refactor",
            Self::Review => "/review",
            Self::Plan => "/plan",
        }
    }

    pub fn info(&self) -> SlashCommandInfo {
        match self {
            Self::Explain => SlashCommandInfo {
                name: "/explain",
                description: "Explain the selected code, architecture, or concept thoroughly.",
                argument_hint: "[code selection or concept]",
                template: "Please explain the following code or concept thoroughly. Break down the architecture, data flow, invariants, and edge cases:\n\n{query}",
            },
            Self::Fix => SlashCommandInfo {
                name: "/fix",
                description: "Identify bugs, type errors, or diagnostics and propose a minimal fix.",
                argument_hint: "[error message or buggy code]",
                template: "Please analyze the following error or problematic code, find the root cause, and propose an exact fix adhering to existing style:\n\n{query}",
            },
            Self::Test => SlashCommandInfo {
                name: "/test",
                description: "Write comprehensive unit, integration, and edge-case tests.",
                argument_hint: "[function or module]",
                template: "Please write comprehensive unit and edge-case tests for the following target. Cover boundary conditions, failure paths, and mock external dependencies cleanly:\n\n{query}",
            },
            Self::Refactor => SlashCommandInfo {
                name: "/refactor",
                description: "Refactor code for cleanliness, idiomatic style, and performance.",
                argument_hint: "[code to refactor]",
                template: "Please refactor the following code to improve readability, maintainability, and efficiency without changing external behavior. Preserve type safety:\n\n{query}",
            },
            Self::Review => SlashCommandInfo {
                name: "/review",
                description: "Adversarial code review for bugs, security, edge cases, and style.",
                argument_hint: "[code or diff to review]",
                template: "Please perform a rigorous, adversarial code review of the following changes or code. Check for logic errors, security vulnerabilities, edge-case regressions, performance issues, and style violations:\n\n{query}",
            },
            Self::Plan => SlashCommandInfo {
                name: "/plan",
                description: "Create an actionable, step-by-step implementation plan.",
                argument_hint: "[feature or refactor request]",
                template: "Please research the codebase and create a detailed, step-by-step implementation plan for the following objective. Cite specific file paths with markdown links and outline verification steps:\n\n{query}",
            },
        }
    }

    pub fn expand(&self, user_query: &str) -> String {
        let info = self.info();
        let query = user_query.trim();
        if query.is_empty() {
            info.template.replace("\n\n{query}", "")
        } else {
            info.template.replace("{query}", query)
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SlashCommandInfo {
    pub name: &'static str,
    pub description: &'static str,
    pub argument_hint: &'static str,
    pub template: &'static str,
}

/// Expands a slash command with user query into a prompt string.
pub fn expand_slash_command(command: &str, user_query: &str) -> Option<String> {
    SlashCommand::from_command_name(command).map(|cmd| cmd.expand(user_query))
}

/// Returns metadata for all supported slash commands.
pub fn list_slash_commands() -> Vec<SlashCommandInfo> {
    vec![
        SlashCommand::Explain.info(),
        SlashCommand::Fix.info(),
        SlashCommand::Test.info(),
        SlashCommand::Refactor.info(),
        SlashCommand::Review.info(),
        SlashCommand::Plan.info(),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_all_six_slash_commands_present() {
        let commands = list_slash_commands();
        assert_eq!(commands.len(), 6);
        let names: Vec<&str> = commands.iter().map(|c| c.name).collect();
        assert!(names.contains(&"/explain"));
        assert!(names.contains(&"/fix"));
        assert!(names.contains(&"/test"));
        assert!(names.contains(&"/refactor"));
        assert!(names.contains(&"/review"));
        assert!(names.contains(&"/plan"));
    }

    #[test]
    fn test_expand_review_and_plan() {
        let rev = expand_slash_command("/review", "fn vulnerable()").unwrap();
        assert!(rev.contains("adversarial code review"));
        assert!(rev.contains("fn vulnerable()"));

        let plan = expand_slash_command("/plan", "Add OAuth login").unwrap();
        assert!(plan.contains("implementation plan"));
        assert!(plan.contains("Add OAuth login"));
    }

    #[test]
    fn test_case_and_slash_insensitivity() {
        assert_eq!(
            expand_slash_command("FIX", "bug").unwrap(),
            expand_slash_command("/fix", "bug").unwrap()
        );
    }
}
