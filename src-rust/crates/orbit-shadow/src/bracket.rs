use crate::error::{Result, ShadowError};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BracketKind {
    Paren,   // ()
    Bracket, // []
    Brace,   // {}
}

impl BracketKind {
    pub fn from_open(c: char) -> Option<Self> {
        match c {
            '(' => Some(BracketKind::Paren),
            '[' => Some(BracketKind::Bracket),
            '{' => Some(BracketKind::Brace),
            _ => None,
        }
    }

    pub fn from_close(c: char) -> Option<Self> {
        match c {
            ')' => Some(BracketKind::Paren),
            ']' => Some(BracketKind::Bracket),
            '}' => Some(BracketKind::Brace),
            _ => None,
        }
    }

    pub fn open_char(self) -> char {
        match self {
            BracketKind::Paren => '(',
            BracketKind::Bracket => '[',
            BracketKind::Brace => '{',
        }
    }

    pub fn close_char(self) -> char {
        match self {
            BracketKind::Paren => ')',
            BracketKind::Bracket => ']',
            BracketKind::Brace => '}',
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            BracketKind::Paren => "parenthesis",
            BracketKind::Bracket => "square bracket",
            BracketKind::Brace => "curly brace",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BracketSpan {
    pub kind: BracketKind,
    pub character: char,
    pub line: usize,
    pub column: usize,
    pub offset: usize,
    pub is_template_expr: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum BracketMismatch {
    Unclosed(BracketSpan),
    UnexpectedClose(BracketSpan),
    MismatchedPair {
        expected: BracketKind,
        actual: BracketSpan,
        opened_at: BracketSpan,
    },
}

impl BracketMismatch {
    pub fn format_message(&self) -> String {
        match self {
            BracketMismatch::Unclosed(span) => {
                format!(
                    "Unclosed {} '{}' opened at line {}:{}",
                    span.kind.name(),
                    span.character,
                    span.line,
                    span.column
                )
            }
            BracketMismatch::UnexpectedClose(span) => {
                format!(
                    "Unexpected closing {} '{}' at line {}:{}",
                    span.kind.name(),
                    span.character,
                    span.line,
                    span.column
                )
            }
            BracketMismatch::MismatchedPair {
                expected,
                actual,
                opened_at,
            } => {
                format!(
                    "Mismatched closing bracket at line {}:{}: expected '{}' to close {} opened at line {}:{}, found '{}'",
                    actual.line,
                    actual.column,
                    expected.close_char(),
                    opened_at.kind.name(),
                    opened_at.line,
                    opened_at.column,
                    actual.character
                )
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct BracketValidationReport {
    pub is_balanced: bool,
    pub mismatches: Vec<BracketMismatch>,
}

impl BracketValidationReport {
    pub fn ensure_balanced(&self, path: &str) -> Result<()> {
        if self.is_balanced {
            Ok(())
        } else {
            let details: Vec<String> = self.mismatches.iter().map(|m| m.format_message()).collect();
            let summary = details.join("; ");
            Err(ShadowError::BracketImbalance {
                path: path.to_string(),
                message: summary,
            })
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Language {
    Rust,
    JavaScript,
    TypeScript,
    Python,
    C,
    Cpp,
    Go,
    Java,
    CSharp,
    Shell,
    Yaml,
    Toml,
    Generic,
}

impl Language {
    pub fn from_path(path: &str) -> Self {
        let p = std::path::Path::new(path);
        let ext = p.extension().and_then(|e| e.to_str()).unwrap_or("");
        match ext.to_ascii_lowercase().as_str() {
            "rs" => Language::Rust,
            "js" | "mjs" | "cjs" | "jsx" => Language::JavaScript,
            "ts" | "mts" | "cts" | "tsx" => Language::TypeScript,
            "py" | "pyw" => Language::Python,
            "c" | "h" => Language::C,
            "cpp" | "cc" | "cxx" | "hpp" | "hh" => Language::Cpp,
            "go" => Language::Go,
            "java" => Language::Java,
            "cs" => Language::CSharp,
            "sh" | "bash" | "zsh" => Language::Shell,
            "yaml" | "yml" => Language::Yaml,
            "toml" => Language::Toml,
            _ => Language::Generic,
        }
    }

    pub fn supports_hash_comments(&self) -> bool {
        matches!(
            self,
            Language::Python | Language::Shell | Language::Yaml | Language::Toml
        )
    }

    pub fn supports_slash_comments(&self) -> bool {
        !self.supports_hash_comments()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ScanMode {
    Normal,
    BacktickString,
}

/// Validates bracket balancing for brackets `()`, `[]`, `{}` across language contexts.
/// Accurately skips string literals, character literals, comments, template strings, and escapes.
pub fn validate_brackets(content: &str, path: Option<&str>) -> BracketValidationReport {
    let lang = path.map(Language::from_path).unwrap_or(Language::Generic);
    let chars: Vec<char> = content.chars().collect();
    let len = chars.len();

    let mut open_brackets: Vec<BracketSpan> = Vec::new();
    let mut mismatches: Vec<BracketMismatch> = Vec::new();
    let mut mode_stack: Vec<ScanMode> = Vec::new();
    let mut current_mode = ScanMode::Normal;

    let mut i = 0;
    let mut line = 1;
    let mut col = 1;
    let mut offset = 0;

    while i < len {
        let c = chars[i];
        let cur_line = line;
        let cur_col = col;
        let cur_offset = offset;

        // Advance line/col tracker for a single char
        let advance_one = |c: char, line: &mut usize, col: &mut usize, offset: &mut usize| {
            if c == '\n' {
                *line += 1;
                *col = 1;
            } else {
                *col += 1;
            }
            *offset += c.len_utf8();
        };

        match current_mode {
            ScanMode::Normal => {
                // 1. Line comments: //
                if lang.supports_slash_comments() && c == '/' && i + 1 < len && chars[i + 1] == '/'
                {
                    advance_one(c, &mut line, &mut col, &mut offset);
                    i += 1;
                    advance_one(chars[i], &mut line, &mut col, &mut offset);
                    i += 1;
                    while i < len && chars[i] != '\n' {
                        advance_one(chars[i], &mut line, &mut col, &mut offset);
                        i += 1;
                    }
                    continue;
                }

                // 2. Line comments: #
                if lang.supports_hash_comments() && c == '#' {
                    advance_one(c, &mut line, &mut col, &mut offset);
                    i += 1;
                    while i < len && chars[i] != '\n' {
                        advance_one(chars[i], &mut line, &mut col, &mut offset);
                        i += 1;
                    }
                    continue;
                }

                // 3. Block comments: /* ... */
                if lang.supports_slash_comments() && c == '/' && i + 1 < len && chars[i + 1] == '*'
                {
                    advance_one(c, &mut line, &mut col, &mut offset);
                    i += 1;
                    advance_one(chars[i], &mut line, &mut col, &mut offset);
                    i += 1;
                    let mut depth = 1;
                    while i < len && depth > 0 {
                        if chars[i] == '/' && i + 1 < len && chars[i + 1] == '*' {
                            advance_one(chars[i], &mut line, &mut col, &mut offset);
                            i += 1;
                            advance_one(chars[i], &mut line, &mut col, &mut offset);
                            i += 1;
                            depth += 1;
                        } else if chars[i] == '*' && i + 1 < len && chars[i + 1] == '/' {
                            advance_one(chars[i], &mut line, &mut col, &mut offset);
                            i += 1;
                            advance_one(chars[i], &mut line, &mut col, &mut offset);
                            i += 1;
                            depth -= 1;
                        } else {
                            advance_one(chars[i], &mut line, &mut col, &mut offset);
                            i += 1;
                        }
                    }
                    continue;
                }

                // 4. Python triple quotes
                if lang == Language::Python
                    && (c == '"' || c == '\'')
                    && i + 2 < len
                    && chars[i + 1] == c
                    && chars[i + 2] == c
                {
                    let quote_char = c;
                    for _ in 0..3 {
                        advance_one(chars[i], &mut line, &mut col, &mut offset);
                        i += 1;
                    }
                    while i + 2 < len {
                        if chars[i] == quote_char
                            && chars[i + 1] == quote_char
                            && chars[i + 2] == quote_char
                        {
                            for _ in 0..3 {
                                advance_one(chars[i], &mut line, &mut col, &mut offset);
                                i += 1;
                            }
                            break;
                        }
                        if chars[i] == '\\' && i + 1 < len {
                            advance_one(chars[i], &mut line, &mut col, &mut offset);
                            i += 1;
                        }
                        advance_one(chars[i], &mut line, &mut col, &mut offset);
                        i += 1;
                    }
                    continue;
                }

                // 5. Double-quoted string literals
                if c == '"' {
                    advance_one(c, &mut line, &mut col, &mut offset);
                    i += 1;
                    let mut in_escape = false;
                    while i < len {
                        let ch = chars[i];
                        advance_one(ch, &mut line, &mut col, &mut offset);
                        i += 1;
                        if in_escape {
                            in_escape = false;
                        } else if ch == '\\' {
                            in_escape = true;
                        } else if ch == '"' {
                            break;
                        } else if ch == '\n'
                            && lang != Language::JavaScript
                            && lang != Language::TypeScript
                        {
                            // Non-JS double quotes rarely span unescaped newlines
                            break;
                        }
                    }
                    continue;
                }

                // 6. Single-quoted char / string literals
                if c == '\'' {
                    // Rust lifetime handling: e.g. 'a, 'static, 'lifetime
                    if lang == Language::Rust
                        && i + 1 < len
                        && (chars[i + 1].is_alphabetic() || chars[i + 1] == '_')
                    {
                        // Check if char literal: 'a' or '\n'
                        let is_char_literal = (i + 2 < len && chars[i + 2] == '\'')
                            || (chars[i + 1] == '\\' && i + 3 < len && chars[i + 3] == '\'');

                        if !is_char_literal {
                            // It's a Rust lifetime: consume ' and identifier
                            advance_one(c, &mut line, &mut col, &mut offset);
                            i += 1;
                            while i < len && (chars[i].is_alphanumeric() || chars[i] == '_') {
                                advance_one(chars[i], &mut line, &mut col, &mut offset);
                                i += 1;
                            }
                            continue;
                        }
                    }

                    advance_one(c, &mut line, &mut col, &mut offset);
                    i += 1;
                    let mut in_escape = false;
                    while i < len {
                        let ch = chars[i];
                        advance_one(ch, &mut line, &mut col, &mut offset);
                        i += 1;
                        if in_escape {
                            in_escape = false;
                        } else if ch == '\\' {
                            in_escape = true;
                        } else if ch == '\'' || ch == '\n' {
                            break;
                        }
                    }
                    continue;
                }

                // 7. Backtick template literals (JS / TS / Generic)
                if c == '`' {
                    advance_one(c, &mut line, &mut col, &mut offset);
                    i += 1;
                    mode_stack.push(current_mode);
                    current_mode = ScanMode::BacktickString;
                    continue;
                }

                // 8. Open brackets
                if let Some(kind) = BracketKind::from_open(c) {
                    advance_one(c, &mut line, &mut col, &mut offset);
                    i += 1;
                    open_brackets.push(BracketSpan {
                        kind,
                        character: c,
                        line: cur_line,
                        column: cur_col,
                        offset: cur_offset,
                        is_template_expr: false,
                    });
                    continue;
                }

                // 9. Close brackets
                if let Some(close_kind) = BracketKind::from_close(c) {
                    advance_one(c, &mut line, &mut col, &mut offset);
                    i += 1;

                    let actual_span = BracketSpan {
                        kind: close_kind,
                        character: c,
                        line: cur_line,
                        column: cur_col,
                        offset: cur_offset,
                        is_template_expr: false,
                    };

                    match open_brackets.pop() {
                        Some(open_span) => {
                            if open_span.kind != close_kind {
                                mismatches.push(BracketMismatch::MismatchedPair {
                                    expected: open_span.kind,
                                    actual: actual_span,
                                    opened_at: open_span,
                                });
                            } else if open_span.is_template_expr {
                                // Returned from a template expression ${ ... }
                                if let Some(prev_mode) = mode_stack.pop() {
                                    current_mode = prev_mode;
                                }
                            }
                        }
                        None => {
                            mismatches.push(BracketMismatch::UnexpectedClose(actual_span));
                        }
                    }
                    continue;
                }

                advance_one(c, &mut line, &mut col, &mut offset);
                i += 1;
            }

            ScanMode::BacktickString => {
                if c == '\\' && i + 1 < len {
                    advance_one(c, &mut line, &mut col, &mut offset);
                    i += 1;
                    advance_one(chars[i], &mut line, &mut col, &mut offset);
                    i += 1;
                    continue;
                }

                // ${ ... } inside template literal
                if c == '$' && i + 1 < len && chars[i + 1] == '{' {
                    advance_one(c, &mut line, &mut col, &mut offset);
                    i += 1;
                    let brace_char = chars[i];
                    let brace_line = line;
                    let brace_col = col;
                    let brace_offset = offset;
                    advance_one(brace_char, &mut line, &mut col, &mut offset);
                    i += 1;

                    open_brackets.push(BracketSpan {
                        kind: BracketKind::Brace,
                        character: brace_char,
                        line: brace_line,
                        column: brace_col,
                        offset: brace_offset,
                        is_template_expr: true,
                    });

                    mode_stack.push(current_mode);
                    current_mode = ScanMode::Normal;
                    continue;
                }

                if c == '`' {
                    advance_one(c, &mut line, &mut col, &mut offset);
                    i += 1;
                    current_mode = mode_stack.pop().unwrap_or(ScanMode::Normal);
                    continue;
                }

                advance_one(c, &mut line, &mut col, &mut offset);
                i += 1;
            }
        }
    }

    // Remaining unclosed brackets at EOF
    for unclosed in open_brackets {
        mismatches.push(BracketMismatch::Unclosed(unclosed));
    }

    BracketValidationReport {
        is_balanced: mismatches.is_empty(),
        mismatches,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_balanced_code() {
        let code = r#"
            fn compute(items: &[i32]) -> Option<Vec<i32>> {
                if items.is_empty() {
                    return None;
                }
                Some(items.iter().map(|x| x * 2).collect())
            }
        "#;
        let report = validate_brackets(code, Some("test.rs"));
        assert!(report.is_balanced);
        assert!(report.mismatches.is_empty());
        assert!(report.ensure_balanced("test.rs").is_ok());
    }

    #[test]
    fn test_brackets_in_strings_and_comments() {
        let code = r#"
            // Here is an unclosed brace: {
            /* And another one: [ with a paren ( */
            let msg = "String with unmatched { and ( and [";
            let ch = '}';
            let paren = ')';
        "#;
        let report = validate_brackets(code, Some("test.rs"));
        assert!(report.is_balanced);
        assert!(report.mismatches.is_empty());
    }

    #[test]
    fn test_rust_lifetimes() {
        let code = r#"
            fn transform<'a, 'b>(input: &'a str) -> &'a str {
                let s = 'x';
                input
            }
        "#;
        let report = validate_brackets(code, Some("src/lib.rs"));
        assert!(report.is_balanced);
    }

    #[test]
    fn test_js_template_literals() {
        let code = r#"
            const greeting = `Hello ${user.name}, you have ${items.map(i => ({ id: i })).length} items!`;
            const nested = `Outer ${ `Inner ${1 + 2}` }`;
        "#;
        let report = validate_brackets(code, Some("index.ts"));
        assert!(report.is_balanced);
    }

    #[test]
    fn test_python_syntax() {
        let code = r#"
            # Unmatched bracket in comment: { ( [
            """
            Docstring with { unclosed brackets [ (
            """
            def calculate(a, b):
                items = [1, 2, (3, 4)]
                return {"result": items}
        "#;
        let report = validate_brackets(code, Some("script.py"));
        assert!(report.is_balanced);
    }

    #[test]
    fn test_unclosed_bracket() {
        let code = "fn foo() {\n    let x = 1;\n";
        let report = validate_brackets(code, Some("foo.rs"));
        assert!(!report.is_balanced);
        assert_eq!(report.mismatches.len(), 1);
        match &report.mismatches[0] {
            BracketMismatch::Unclosed(span) => {
                assert_eq!(span.kind, BracketKind::Brace);
                assert_eq!(span.line, 1);
            }
            _ => panic!("Expected unclosed bracket"),
        }
        assert!(report.ensure_balanced("foo.rs").is_err());
    }

    #[test]
    fn test_unexpected_closing_bracket() {
        let code = "let x = 1;\n}\n";
        let report = validate_brackets(code, Some("foo.rs"));
        assert!(!report.is_balanced);
        assert_eq!(report.mismatches.len(), 1);
        match &report.mismatches[0] {
            BracketMismatch::UnexpectedClose(span) => {
                assert_eq!(span.kind, BracketKind::Brace);
                assert_eq!(span.line, 2);
            }
            _ => panic!("Expected unexpected close"),
        }
    }

    #[test]
    fn test_mismatched_pair() {
        let code = "let items = (1, 2, 3];";
        let report = validate_brackets(code, Some("foo.rs"));
        assert!(!report.is_balanced);
        assert_eq!(report.mismatches.len(), 1);
        match &report.mismatches[0] {
            BracketMismatch::MismatchedPair {
                expected, actual, ..
            } => {
                assert_eq!(*expected, BracketKind::Paren);
                assert_eq!(actual.kind, BracketKind::Bracket);
            }
            _ => panic!("Expected mismatched pair"),
        }
    }
}
