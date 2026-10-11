use serde::{Deserialize, Serialize};
use serde_json::json;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolParameter {
    pub name: String,
    pub param_type: String,
    pub description: String,
    pub required: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolDefinition {
    pub name: String,
    pub description: String,
    pub parameters: Vec<ToolParameter>,
    pub example: String,
    pub is_read_only: bool,
}

impl ToolDefinition {
    pub fn input_schema(&self) -> serde_json::Value {
        let mut properties = serde_json::Map::new();
        let mut required = Vec::new();

        for p in &self.parameters {
            properties.insert(
                p.name.clone(),
                json!({
                    "type": p.param_type,
                    "description": p.description,
                }),
            );
            if p.required {
                required.push(json!(p.name));
            }
        }

        json!({
            "type": "object",
            "properties": properties,
            "required": required,
        })
    }

    pub fn to_xml_definition(&self) -> String {
        let mut out = String::with_capacity(1024);
        out.push_str(&format!("<tool_definition name=\"{}\">\n", self.name));
        out.push_str("<description>\n");
        out.push_str(self.description.trim());
        out.push_str("\n</description>\n");

        out.push_str("<parameters>\n");
        for p in &self.parameters {
            out.push_str(&format!(
                "  <parameter name=\"{}\" type=\"{}\" required=\"{}\">\n    {}\n  </parameter>\n",
                p.name,
                p.param_type,
                p.required,
                p.description.trim()
            ));
        }
        out.push_str("</parameters>\n");

        if !self.example.is_empty() {
            out.push_str("<example>\n");
            out.push_str(self.example.trim());
            out.push_str("\n</example>\n");
        }

        out.push_str("</tool_definition>");
        out
    }
}

/// Returns the built-in Read file tool definition.
pub fn read_tool() -> ToolDefinition {
    ToolDefinition {
        name: "Read".to_string(),
        description: r#"Reads a file from the local filesystem. You can access any file directly by using this tool.
If the User provides a path to a file assume that path is valid. It is okay to read a file that does not exist; an error will be returned.

Usage:
- You can optionally specify a line offset and limit (especially handy for long files), but it's recommended to read the whole file by not providing these parameters.
- Lines in the output are numbered starting at 1, using following format: LINE_NUMBER|LINE_CONTENT
- You have the capability to call multiple tools in a single response. Speculatively read multiple files as a batch when useful.
- If you read a file that exists but has empty contents you will receive 'File is empty.'

Image Support:
- Supports jpeg/jpg, png, gif, webp image inspection.

PDF Support:
- PDF files are converted into readable text content automatically."#.to_string(),
        parameters: vec![
            ToolParameter {
                name: "path".to_string(),
                param_type: "string".to_string(),
                description: "The absolute or workspace-relative path to the file.".to_string(),
                required: true,
            },
            ToolParameter {
                name: "offset".to_string(),
                param_type: "integer".to_string(),
                description: "Optional. Line number to start reading from. Positive values are 1-indexed. Negative values count backwards from end.".to_string(),
                required: false,
            },
            ToolParameter {
                name: "limit".to_string(),
                param_type: "integer".to_string(),
                description: "Optional. Number of lines to read.".to_string(),
                required: false,
            },
        ],
        example: r#"<Read>
<path>src/main.rs</path>
</Read>

Read tail of file:
<Read>
<path>server.log</path>
<offset>-50</offset>
<limit>50</limit>
</Read>"#.to_string(),
        is_read_only: true,
    }
}

/// Returns the built-in Write file tool definition.
pub fn write_tool() -> ToolDefinition {
    ToolDefinition {
        name: "Write".to_string(),
        description: r#"Writes full text content to a file on the local filesystem.
Creates parent directories automatically if they do not exist.
Completely replaces existing file contents. Use this tool when creating new files or when an entire file rewrite is appropriate."#.to_string(),
        parameters: vec![
            ToolParameter {
                name: "path".to_string(),
                param_type: "string".to_string(),
                description: "The absolute or workspace-relative path to the file to create or overwrite.".to_string(),
                required: true,
            },
            ToolParameter {
                name: "content".to_string(),
                param_type: "string".to_string(),
                description: "The full content to write to the file.".to_string(),
                required: true,
            },
        ],
        example: r#"<Write>
<path>src/config.json</path>
<content>
{
  "version": "1.0",
  "debug": false
}
</content>
</Write>"#.to_string(),
        is_read_only: false,
    }
}

/// Returns the built-in StrReplace / Edit tool definition.
pub fn edit_tool() -> ToolDefinition {
    ToolDefinition {
        name: "StrReplace".to_string(),
        description: r#"Performs a targeted search-and-replace edit on a file.
Replaces `old_str` with `new_str` in the specified file.
The `old_str` must match the file contents EXACTLY byte-for-byte, including all whitespace, indentation, comments, and line endings.
`old_str` must occur uniquely within the file. If it occurs more than once, expand `old_str` with surrounding context lines until it is unique."#.to_string(),
        parameters: vec![
            ToolParameter {
                name: "path".to_string(),
                param_type: "string".to_string(),
                description: "The absolute or workspace-relative path to the file to modify.".to_string(),
                required: true,
            },
            ToolParameter {
                name: "old_str".to_string(),
                param_type: "string".to_string(),
                description: "The exact text snippet currently in the file to be replaced.".to_string(),
                required: true,
            },
            ToolParameter {
                name: "new_str".to_string(),
                param_type: "string".to_string(),
                description: "The replacement text to insert in place of old_str.".to_string(),
                required: true,
            },
        ],
        example: r#"<StrReplace>
<path>src/lib.rs</path>
<old_str>
fn calculate() -> i32 {
    0
}
</old_str>
<new_str>
fn calculate() -> i32 {
    42
}
</new_str>
</StrReplace>"#.to_string(),
        is_read_only: false,
    }
}

/// Returns the built-in Shell / Terminal command tool definition.
pub fn terminal_tool() -> ToolDefinition {
    ToolDefinition {
        name: "Shell".to_string(),
        description: r#"Runs shell commands on the local machine in a persistent shell session.
Allows running builds, package managers, git commands, test runners, and diagnostic commands.
Always prefer non-interactive flags (e.g. -y, --yes) to avoid blocking on prompts.
Set `is_background: true` only for long-running watchers, servers, or daemons."#.to_string(),
        parameters: vec![
            ToolParameter {
                name: "command".to_string(),
                param_type: "string".to_string(),
                description: "The shell command to execute.".to_string(),
                required: true,
            },
            ToolParameter {
                name: "cwd".to_string(),
                param_type: "string".to_string(),
                description: "Optional. Working directory to execute the command in.".to_string(),
                required: false,
            },
            ToolParameter {
                name: "is_background".to_string(),
                param_type: "boolean".to_string(),
                description: "Optional. Set to true to run command in background without blocking.".to_string(),
                required: false,
            },
            ToolParameter {
                name: "timeout_ms".to_string(),
                param_type: "integer".to_string(),
                description: "Optional. Maximum milliseconds to wait for foreground execution (default 30000).".to_string(),
                required: false,
            },
        ],
        example: r#"<Shell>
<command>cargo test --all</command>
</Shell>"#.to_string(),
        is_read_only: false,
    }
}

/// Returns the built-in Web Search tool definition.
pub fn web_search_tool() -> ToolDefinition {
    ToolDefinition {
        name: "WebSearch".to_string(),
        description: r#"Searches the web for up-to-date documentation, API references, library releases, or bug solutions.
Returns title, URL, and text snippets for the top search results."#.to_string(),
        parameters: vec![
            ToolParameter {
                name: "query".to_string(),
                param_type: "string".to_string(),
                description: "The search query keywords or phrase.".to_string(),
                required: true,
            },
            ToolParameter {
                name: "limit".to_string(),
                param_type: "integer".to_string(),
                description: "Optional. Maximum number of search results to return (default: 5).".to_string(),
                required: false,
            },
        ],
        example: r#"<WebSearch>
<query>tokio broadcast channel lag error</query>
</WebSearch>"#.to_string(),
        is_read_only: true,
    }
}

/// Returns the built-in Web Fetch tool definition.
pub fn web_fetch_tool() -> ToolDefinition {
    ToolDefinition {
        name: "WebFetch".to_string(),
        description: r#"Fetches content from a URL on the internet.
Extracts clean Markdown/text content without browser rendering overhead.
Works with web pages, markdown documents, and raw text files."#
            .to_string(),
        parameters: vec![
            ToolParameter {
                name: "url".to_string(),
                param_type: "string".to_string(),
                description: "The HTTP/HTTPS URL to fetch content from.".to_string(),
                required: true,
            },
            ToolParameter {
                name: "char_limit".to_string(),
                param_type: "integer".to_string(),
                description: "Optional. Character cap for returned content (default: 20000)."
                    .to_string(),
                required: false,
            },
        ],
        example: r#"<WebFetch>
<url>https://docs.rs/tokio/latest/tokio/sync/broadcast/index.html</url>
</WebFetch>"#
            .to_string(),
        is_read_only: true,
    }
}

/// Returns the built-in Glob tool definition.
pub fn glob_tool() -> ToolDefinition {
    ToolDefinition {
        name: "Glob".to_string(),
        description: r#"Fast file search matching a glob pattern across the codebase.
Returns matching file paths sorted by modification time.
Patterns not starting with "**/" are automatically treated as recursive."#
            .to_string(),
        parameters: vec![
            ToolParameter {
                name: "glob_pattern".to_string(),
                param_type: "string".to_string(),
                description:
                    "Glob pattern to match files against (e.g. '**/*.rs', 'src/**/*.tsx')."
                        .to_string(),
                required: true,
            },
            ToolParameter {
                name: "target_directory".to_string(),
                param_type: "string".to_string(),
                description: "Optional directory to search in. Defaults to workspace root."
                    .to_string(),
                required: false,
            },
        ],
        example: r#"<Glob>
<glob_pattern>crates/**/*.rs</glob_pattern>
</Glob>"#
            .to_string(),
        is_read_only: true,
    }
}

/// Returns the built-in Grep tool definition.
pub fn grep_tool() -> ToolDefinition {
    ToolDefinition {
        name: "Grep".to_string(),
        description: r#"High-performance regex content search built on ripgrep.
Supports regex patterns, file filtering by glob, and case-sensitive/insensitive matching.
Prefer Grep over running rg via Shell whenever possible."#
            .to_string(),
        parameters: vec![
            ToolParameter {
                name: "pattern".to_string(),
                param_type: "string".to_string(),
                description: "Regular expression pattern to search for in file contents."
                    .to_string(),
                required: true,
            },
            ToolParameter {
                name: "path".to_string(),
                param_type: "string".to_string(),
                description: "Optional directory or file path to restrict search to.".to_string(),
                required: false,
            },
            ToolParameter {
                name: "glob".to_string(),
                param_type: "string".to_string(),
                description: "Optional glob pattern to filter files (e.g. '*.rs').".to_string(),
                required: false,
            },
        ],
        example: r#"<Grep>
<pattern>pub fn generate_system_prompt</pattern>
<glob>*.rs</glob>
</Grep>"#
            .to_string(),
        is_read_only: true,
    }
}

/// Returns the built-in TodoWrite tool definition.
pub fn todo_tool() -> ToolDefinition {
    ToolDefinition {
        name: "TodoWrite".to_string(),
        description: r#"Manages the task list for complex multi-step tasks.
Tracks pending, in_progress, and completed todos to keep the user and agent aligned."#.to_string(),
        parameters: vec![
            ToolParameter {
                name: "todos".to_string(),
                param_type: "array".to_string(),
                description: "Array of todo items with id, content, and status (pending|in_progress|completed).".to_string(),
                required: true,
            },
        ],
        example: r#"<TodoWrite>
<todos>[{"id": "1", "content": "Extract prompts", "status": "completed"}, {"id": "2", "content": "Add tests", "status": "in_progress"}]</todos>
</TodoWrite>"#.to_string(),
        is_read_only: false,
    }
}

/// Returns the built-in AskQuestion tool definition.
pub fn ask_question_tool() -> ToolDefinition {
    ToolDefinition {
        name: "AskQuestion".to_string(),
        description: r#"Presents structured multiple-choice questions or options to the user.
Use when multiple valid implementations exist, when blocked, or when user input is required."#
            .to_string(),
        parameters: vec![
            ToolParameter {
                name: "question".to_string(),
                param_type: "string".to_string(),
                description: "The question or decision prompt to show the user.".to_string(),
                required: true,
            },
            ToolParameter {
                name: "options".to_string(),
                param_type: "array".to_string(),
                description: "Array of choice strings for the user to select from.".to_string(),
                required: true,
            },
        ],
        example: r#"<AskQuestion>
<question>Which database driver should we configure?</question>
<options>["PostgreSQL (sqlx)", "SQLite (bundled)", "In-memory"]</options>
</AskQuestion>"#
            .to_string(),
        is_read_only: true,
    }
}

/// Returns the built-in CreatePlan tool definition.
pub fn create_plan_tool() -> ToolDefinition {
    ToolDefinition {
        name: "create_plan".to_string(),
        description: r#"Creates and submits a formal implementation plan to the user for review.
Must be called in Plan mode before any mutating actions are taken."#
            .to_string(),
        parameters: vec![ToolParameter {
            name: "plan".to_string(),
            param_type: "string".to_string(),
            description: "Full markdown plan content detailing steps, files, and verification."
                .to_string(),
            required: true,
        }],
        example: r#"<create_plan>
<plan>
# Migration Plan
1. [src/lib.rs](src/lib.rs): Add module
2. Verify with cargo test
</plan>
</create_plan>"#
            .to_string(),
        is_read_only: true,
    }
}

/// Returns the complete catalog of Orbit built-in tools.
pub fn builtin_tool_catalog() -> Vec<ToolDefinition> {
    vec![
        read_tool(),
        write_tool(),
        edit_tool(),
        terminal_tool(),
        web_search_tool(),
        web_fetch_tool(),
        glob_tool(),
        grep_tool(),
        todo_tool(),
        ask_question_tool(),
        create_plan_tool(),
    ]
}

/// Formats a list of tool definitions into XML for inclusion in system prompts.
pub fn format_tools_xml_definitions(tools: &[ToolDefinition]) -> String {
    let mut out = String::with_capacity(tools.len() * 1024);
    out.push_str("<tool_definitions>\n");
    for tool in tools {
        out.push_str(&tool.to_xml_definition());
        out.push_str("\n\n");
    }
    out.push_str("</tool_definitions>");
    out
}

/// Formats an XML tool call snippet.
pub fn format_tool_call_xml(tool_name: &str, params: &[(&str, &str)]) -> String {
    let mut out = format!("<{}>\n", tool_name);
    for (k, v) in params {
        out.push_str(&format!("<{}>{}</{}>\n", k, v.trim(), k));
    }
    out.push_str(&format!("</{}>", tool_name));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_builtin_tool_catalog_includes_all_requested_tools() {
        let tools = builtin_tool_catalog();
        let names: Vec<&str> = tools.iter().map(|t| t.name.as_str()).collect();

        assert!(names.contains(&"Read"));
        assert!(names.contains(&"Write"));
        assert!(names.contains(&"StrReplace"));
        assert!(names.contains(&"Shell"));
        assert!(names.contains(&"WebSearch"));
        assert!(names.contains(&"WebFetch"));
    }

    #[test]
    fn test_tool_input_schema_generation() {
        let read = read_tool();
        let schema = read.input_schema();
        assert_eq!(schema["type"], "object");
        assert!(schema["properties"]["path"].is_object());
        assert_eq!(schema["required"], json!(["path"]));
    }

    #[test]
    fn test_format_tool_call_xml() {
        let call = format_tool_call_xml(
            "StrReplace",
            &[
                ("path", "src/main.rs"),
                ("old_str", "let a = 1;"),
                ("new_str", "let a = 2;"),
            ],
        );
        assert!(call.starts_with("<StrReplace>\n"));
        assert!(call.contains("<path>src/main.rs</path>"));
        assert!(call.contains("<old_str>let a = 1;</old_str>"));
        assert!(call.ends_with("</StrReplace>"));
    }
}
