use globset::{GlobBuilder, GlobSet, GlobSetBuilder};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum RulesError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Invalid glob pattern '{pattern}': {source}")]
    InvalidGlob {
        pattern: String,
        source: globset::Error,
    },

    #[error("GlobSet build error: {0}")]
    GlobSet(#[from] globset::Error),

    #[error("Frontmatter syntax error: {0}")]
    ParseError(String),

    #[error("Rule error: {0}")]
    Other(String),
}

impl From<RulesError> for orbit_common::OrbitError {
    fn from(err: RulesError) -> Self {
        match err {
            RulesError::Io(io_err) => orbit_common::OrbitError::Io(io_err),
            other => orbit_common::OrbitError::InvalidState(other.to_string()),
        }
    }
}

pub type Result<T> = std::result::Result<T, RulesError>;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuleFrontmatter {
    pub description: String,
    pub globs: Vec<String>,
    pub always_apply: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Rule {
    pub name: String,
    pub description: String,
    pub globs: Vec<String>,
    pub always_apply: bool,
    pub content: String,
    pub source_path: Option<PathBuf>,
}

impl Rule {
    pub fn new(
        name: impl Into<String>,
        description: impl Into<String>,
        globs: Vec<String>,
        always_apply: bool,
        content: impl Into<String>,
    ) -> Self {
        Self {
            name: name.into(),
            description: description.into(),
            globs,
            always_apply,
            content: content.into(),
            source_path: None,
        }
    }

    pub fn with_source(mut self, path: impl Into<PathBuf>) -> Self {
        self.source_path = Some(path.into());
        self
    }
}

pub struct RuleParser;

impl RuleParser {
    pub fn parse(content: &str) -> Result<Rule> {
        Self::parse_with_name("rule", content)
    }

    pub fn parse_with_name(name: &str, content: &str) -> Result<Rule> {
        let (fm_opt, body_str) = split_frontmatter(content)?;
        let (description, globs, always_apply) = match fm_opt {
            Some(fm) => parse_frontmatter(fm),
            None => (String::new(), Vec::new(), false),
        };

        Ok(Rule {
            name: name.to_string(),
            description,
            globs,
            always_apply,
            content: body_str.trim().to_string(),
            source_path: None,
        })
    }

    pub fn parse_file<P: AsRef<Path>>(path: P) -> Result<Rule> {
        let p = path.as_ref();
        let content = std::fs::read_to_string(p)?;
        let name = p.file_stem().and_then(|s| s.to_str()).unwrap_or("rule");
        let mut rule = Self::parse_with_name(name, &content)?;
        rule.source_path = Some(p.to_path_buf());
        Ok(rule)
    }

    pub fn parse_legacy_cursorrules(content: &str) -> Result<Rule> {
        let (fm_opt, body_str) = split_frontmatter(content)?;
        let (description, globs, always_apply) = match fm_opt {
            Some(fm) => {
                let (desc, gls, mut aa) = parse_frontmatter(fm);
                if !fm.contains("alwaysApply") && !fm.contains("always_apply") {
                    aa = true;
                }
                (desc, gls, aa)
            }
            None => ("Legacy .cursorrules".to_string(), Vec::new(), true),
        };

        Ok(Rule {
            name: ".cursorrules".to_string(),
            description,
            globs,
            always_apply,
            content: body_str.trim().to_string(),
            source_path: None,
        })
    }
}

// ponytail: minimal line-based YAML frontmatter parser; upgrade to serde_yaml if nested mapping trees or anchors are required.
fn split_frontmatter(input: &str) -> Result<(Option<&str>, &str)> {
    let clean = input.trim_start_matches('\u{feff}');
    let trimmed = clean.trim_start();
    if !trimmed.starts_with("---") {
        return Ok((None, clean));
    }

    let after_open = match trimmed.find('\n') {
        Some(pos) => &trimmed[pos + 1..],
        None => {
            return Ok((Some(""), ""));
        }
    };

    let mut offset = 0;
    for line in after_open.split_inclusive('\n') {
        let trimmed_line = line.trim();
        if trimmed_line == "---" || trimmed_line == "..." {
            let fm_content = &after_open[..offset];
            let body_content = &after_open[offset + line.len()..];
            return Ok((Some(fm_content), body_content));
        }
        offset += line.len();
    }

    Err(RulesError::ParseError(
        "Unclosed YAML frontmatter: missing closing delimiter '---'".to_string(),
    ))
}

fn parse_frontmatter(fm_str: &str) -> (String, Vec<String>, bool) {
    let mut description = String::new();
    let mut globs = Vec::new();
    let mut always_apply = false;
    let mut in_globs_list = false;

    for raw_line in fm_str.lines() {
        let line = raw_line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }

        if in_globs_list {
            if let Some(item) = line.strip_prefix('-') {
                let glob = clean_quote_and_whitespace(item);
                if !glob.is_empty() {
                    globs.push(glob);
                }
                continue;
            } else if !line.contains(':') {
                continue;
            } else {
                in_globs_list = false;
            }
        }

        if let Some((key, val)) = line.split_once(':') {
            let key = key.trim().to_ascii_lowercase();
            let val = val.trim();
            match key.as_str() {
                "description" => {
                    in_globs_list = false;
                    description = clean_quote_and_whitespace(val);
                }
                "alwaysapply" | "always_apply" => {
                    in_globs_list = false;
                    always_apply = parse_bool(val);
                }
                "globs" => {
                    if val.is_empty() {
                        in_globs_list = true;
                    } else {
                        in_globs_list = false;
                        globs.extend(parse_globs_inline(val));
                    }
                }
                _ => {
                    in_globs_list = false;
                }
            }
        }
    }

    (description, globs, always_apply)
}

fn parse_globs_inline(val: &str) -> Vec<String> {
    let trimmed = val.trim();
    if trimmed.is_empty() {
        return Vec::new();
    }

    if trimmed.starts_with('[') && trimmed.ends_with(']') {
        if let Ok(list) = serde_json::from_str::<Vec<String>>(trimmed) {
            return list
                .into_iter()
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect();
        }
        let inner = &trimmed[1..trimmed.len() - 1];
        return inner
            .split(',')
            .map(clean_quote_and_whitespace)
            .filter(|s| !s.is_empty())
            .collect();
    }

    if trimmed.contains(',') {
        return trimmed
            .split(',')
            .map(clean_quote_and_whitespace)
            .filter(|s| !s.is_empty())
            .collect();
    }

    let single = clean_quote_and_whitespace(trimmed);
    if single.is_empty() {
        Vec::new()
    } else {
        vec![single]
    }
}

fn clean_quote_and_whitespace(s: &str) -> String {
    let s = s.trim();
    if (s.starts_with('"') && s.ends_with('"') && s.len() >= 2)
        || (s.starts_with('\'') && s.ends_with('\'') && s.len() >= 2)
    {
        s[1..s.len() - 1].trim().to_string()
    } else {
        s.to_string()
    }
}

fn parse_bool(s: &str) -> bool {
    let cleaned = clean_quote_and_whitespace(s).to_ascii_lowercase();
    matches!(cleaned.as_str(), "true" | "1" | "yes" | "on")
}

#[derive(Debug, Clone, Default)]
pub struct RulesRegistry {
    rules: Vec<Rule>,
}

impl RulesRegistry {
    pub fn new() -> Self {
        Self { rules: Vec::new() }
    }

    pub fn with_rules(rules: Vec<Rule>) -> Self {
        Self { rules }
    }

    pub fn scan_workspace<P: AsRef<Path>>(workspace_root: P) -> Result<Self> {
        let mut registry = Self::new();
        registry.load_from_workspace(workspace_root)?;
        Ok(registry)
    }

    pub fn load_from_workspace<P: AsRef<Path>>(&mut self, workspace_root: P) -> Result<usize> {
        let root = workspace_root.as_ref();
        let initial_count = self.rules.len();

        // 1. Scan .cursor/rules/*.mdc
        let cursor_dir = root.join(".cursor").join("rules");
        self.scan_directory(&cursor_dir)?;

        // 2. Scan .orbit/rules/*.mdc
        let orbit_dir = root.join(".orbit").join("rules");
        self.scan_directory(&orbit_dir)?;

        // 3. Scan legacy .cursorrules
        let cursorrules_path = root.join(".cursorrules");
        if cursorrules_path.is_file() {
            let content = std::fs::read_to_string(&cursorrules_path)?;
            let mut rule = RuleParser::parse_legacy_cursorrules(&content)?;
            rule.source_path = Some(cursorrules_path);
            self.upsert_rule(rule);
        }

        Ok(self.rules.len() - initial_count)
    }

    fn scan_directory(&mut self, dir: &Path) -> Result<()> {
        if !dir.is_dir() {
            return Ok(());
        }

        let mut dirs_to_visit = vec![dir.to_path_buf()];
        while let Some(current_dir) = dirs_to_visit.pop() {
            let read_dir = match std::fs::read_dir(&current_dir) {
                Ok(rd) => rd,
                Err(err) if err.kind() == std::io::ErrorKind::NotFound => continue,
                Err(err) => return Err(RulesError::Io(err)),
            };

            let mut file_entries = Vec::new();
            for entry_res in read_dir {
                let entry = entry_res?;
                let path = entry.path();
                if path.is_dir() {
                    dirs_to_visit.push(path);
                } else if path.is_file() {
                    let is_mdc = path
                        .extension()
                        .and_then(|ext| ext.to_str())
                        .map(|ext| ext.eq_ignore_ascii_case("mdc"))
                        .unwrap_or(false);

                    if is_mdc {
                        file_entries.push(path);
                    }
                }
            }

            file_entries.sort();
            for path in file_entries {
                let rule = RuleParser::parse_file(&path)?;
                self.upsert_rule(rule);
            }
        }

        Ok(())
    }

    pub fn upsert_rule(&mut self, rule: Rule) {
        if let Some(existing) = self.rules.iter_mut().find(|r| r.name == rule.name) {
            *existing = rule;
        } else {
            self.rules.push(rule);
        }
    }

    pub fn add_rule(&mut self, rule: Rule) {
        self.upsert_rule(rule);
    }

    pub fn rules(&self) -> &[Rule] {
        &self.rules
    }

    pub fn get_rule(&self, name: &str) -> Option<&Rule> {
        self.rules.iter().find(|r| r.name == name)
    }

    pub fn len(&self) -> usize {
        self.rules.len()
    }

    pub fn is_empty(&self) -> bool {
        self.rules.is_empty()
    }
}

pub struct RuleMatcher {
    rules: Vec<Rule>,
    glob_set: GlobSet,
    glob_to_rule: Vec<usize>,
    always_apply_indices: Vec<usize>,
}

impl RuleMatcher {
    pub fn new(rules: Vec<Rule>) -> Result<Self> {
        let mut builder = GlobSetBuilder::new();
        let mut glob_to_rule = Vec::new();
        let mut always_apply_indices = Vec::new();

        for (rule_idx, rule) in rules.iter().enumerate() {
            if rule.always_apply {
                always_apply_indices.push(rule_idx);
            }

            for pattern in &rule.globs {
                let pat = pattern.trim();
                if pat.is_empty() {
                    continue;
                }

                let normalized_pat = pat.trim_start_matches("./");
                let glob = GlobBuilder::new(normalized_pat)
                    .literal_separator(false)
                    .build()
                    .map_err(|e| RulesError::InvalidGlob {
                        pattern: pat.to_string(),
                        source: e,
                    })?;

                builder.add(glob);
                glob_to_rule.push(rule_idx);
            }
        }

        let glob_set = builder.build()?;

        Ok(Self {
            rules,
            glob_set,
            glob_to_rule,
            always_apply_indices,
        })
    }

    pub fn from_registry(registry: &RulesRegistry) -> Result<Self> {
        Self::new(registry.rules().to_vec())
    }

    pub fn rules(&self) -> &[Rule] {
        &self.rules
    }

    pub fn match_path<P: AsRef<Path>>(&self, path: P) -> Vec<&Rule> {
        let raw = path.as_ref().to_string_lossy();
        let normalized = normalize_path(&raw);

        let mut matched_indices = BTreeSet::new();

        for &idx in &self.always_apply_indices {
            matched_indices.insert(idx);
        }

        for glob_idx in self.glob_set.matches(&normalized) {
            if let Some(&rule_idx) = self.glob_to_rule.get(glob_idx) {
                matched_indices.insert(rule_idx);
            }
        }

        if let Some(file_name) = Path::new(&normalized).file_name().and_then(|f| f.to_str()) {
            if file_name != normalized {
                for glob_idx in self.glob_set.matches(file_name) {
                    if let Some(&rule_idx) = self.glob_to_rule.get(glob_idx) {
                        matched_indices.insert(rule_idx);
                    }
                }
            }
        }

        matched_indices
            .into_iter()
            .filter_map(|idx| self.rules.get(idx))
            .collect()
    }

    pub fn match_workspace_file<P1: AsRef<Path>, P2: AsRef<Path>>(
        &self,
        workspace_root: P1,
        file_path: P2,
    ) -> Vec<&Rule> {
        let root = workspace_root.as_ref();
        let file = file_path.as_ref();
        let rel = file.strip_prefix(root).unwrap_or(file);
        self.match_path(rel)
    }

    pub fn matches_rule<P: AsRef<Path>>(&self, rule: &Rule, path: P) -> bool {
        if rule.always_apply {
            return true;
        }

        let raw = path.as_ref().to_string_lossy();
        let normalized = normalize_path(&raw);

        for pat in &rule.globs {
            let pat_norm = pat.trim().trim_start_matches("./");
            if let Ok(glob) = GlobBuilder::new(pat_norm).literal_separator(false).build() {
                let matcher = glob.compile_matcher();
                if matcher.is_match(&normalized) {
                    return true;
                }
                if let Some(file_name) = Path::new(&normalized).file_name().and_then(|f| f.to_str())
                {
                    if file_name != normalized && matcher.is_match(file_name) {
                        return true;
                    }
                }
            }
        }

        false
    }

    pub fn always_apply_rules(&self) -> Vec<&Rule> {
        self.always_apply_indices
            .iter()
            .filter_map(|&idx| self.rules.get(idx))
            .collect()
    }
}

fn normalize_path(path_str: &str) -> String {
    let s = path_str.replace('\\', "/");
    let trimmed = s.trim_start_matches("./");
    trimmed.to_string()
}

pub struct PromptSectionBuilder<'a> {
    rules: Vec<&'a Rule>,
    wrap_tags: bool,
}

impl<'a> PromptSectionBuilder<'a> {
    pub fn new() -> Self {
        Self {
            rules: Vec::new(),
            wrap_tags: true,
        }
    }

    pub fn from_rules(rules: &'a [Rule]) -> Self {
        Self {
            rules: rules.iter().collect(),
            wrap_tags: true,
        }
    }

    pub fn with_rules<I: IntoIterator<Item = &'a Rule>>(rules: I) -> Self {
        Self {
            rules: rules.into_iter().collect(),
            wrap_tags: true,
        }
    }

    pub fn add_rule(&mut self, rule: &'a Rule) -> &mut Self {
        self.rules.push(rule);
        self
    }

    pub fn wrap_tags(&mut self, wrap: bool) -> &mut Self {
        self.wrap_tags = wrap;
        self
    }

    pub fn is_empty(&self) -> bool {
        self.rules.is_empty()
    }

    pub fn len(&self) -> usize {
        self.rules.len()
    }

    pub fn build(&self) -> String {
        if self.rules.is_empty() {
            return String::new();
        }

        let mut out = String::with_capacity(4096);
        out.push_str("<cursor_rules>\n");

        for (i, rule) in self.rules.iter().enumerate() {
            if i > 0 {
                out.push('\n');
            }
            if self.wrap_tags {
                out.push_str("<rule name=\"");
                out.push_str(&escape_xml_attr(&rule.name));
                out.push('"');
                if !rule.description.trim().is_empty() {
                    out.push_str(" description=\"");
                    out.push_str(&escape_xml_attr(&rule.description));
                    out.push('"');
                }
                out.push_str(">\n");
                out.push_str(rule.content.trim());
                out.push_str("\n</rule>\n");
            } else {
                out.push_str(rule.content.trim());
                out.push('\n');
            }
        }

        out.push_str("</cursor_rules>\n");
        out
    }

    pub fn build_option(&self) -> Option<String> {
        if self.rules.is_empty() {
            None
        } else {
            Some(self.build())
        }
    }
}

impl<'a> Default for PromptSectionBuilder<'a> {
    fn default() -> Self {
        Self::new()
    }
}

fn escape_xml_attr(s: &str) -> String {
    let mut escaped = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => escaped.push_str("&amp;"),
            '<' => escaped.push_str("&lt;"),
            '>' => escaped.push_str("&gt;"),
            '"' => escaped.push_str("&quot;"),
            '\'' => escaped.push_str("&apos;"),
            _ => escaped.push(c),
        }
    }
    escaped
}

pub fn format_cursor_rules(rules: &[&Rule]) -> String {
    PromptSectionBuilder::with_rules(rules.iter().copied()).build()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn test_rule_parser_standard_frontmatter() {
        let content = r#"---
description: "TypeScript and React formatting"
globs: ["*.ts", "src/**/*.tsx"]
alwaysApply: false
---
# React Guidelines
Always use function components.
"#;
        let rule = RuleParser::parse_with_name("react-rules", content).expect("parse failed");
        assert_eq!(rule.name, "react-rules");
        assert_eq!(rule.description, "TypeScript and React formatting");
        assert_eq!(rule.globs, vec!["*.ts", "src/**/*.tsx"]);
        assert!(!rule.always_apply);
        assert_eq!(
            rule.content,
            "# React Guidelines\nAlways use function components."
        );
    }

    #[test]
    fn test_rule_parser_always_apply_only() {
        let content = r#"---
alwaysApply: true
---
# Safety Rules
Verify before editing.
"#;
        let rule = RuleParser::parse(content).expect("parse failed");
        assert_eq!(rule.name, "rule");
        assert_eq!(rule.description, "");
        assert!(rule.globs.is_empty());
        assert!(rule.always_apply);
        assert_eq!(rule.content, "# Safety Rules\nVerify before editing.");
    }

    #[test]
    fn test_rule_parser_yaml_sequence_globs() {
        let content = r#"---
description: Python rules
globs:
  - "*.py"
  - "scripts/**/*.sh"
always_apply: true
---
Use ruff and black.
"#;
        let rule = RuleParser::parse_with_name("py-rule", content).expect("parse failed");
        assert_eq!(rule.name, "py-rule");
        assert_eq!(rule.description, "Python rules");
        assert_eq!(rule.globs, vec!["*.py", "scripts/**/*.sh"]);
        assert!(rule.always_apply);
        assert_eq!(rule.content, "Use ruff and black.");
    }

    #[test]
    fn test_rule_parser_unquoted_and_single_glob() {
        let content = r#"---
description: Rust clippy rules
globs: *.rs
alwaysApply: false
---
No warnings allowed.
"#;
        let rule = RuleParser::parse(content).expect("parse failed");
        assert_eq!(rule.description, "Rust clippy rules");
        assert_eq!(rule.globs, vec!["*.rs"]);
        assert!(!rule.always_apply);
        assert_eq!(rule.content, "No warnings allowed.");
    }

    #[test]
    fn test_rule_parser_no_frontmatter() {
        let content = "Just raw markdown content without frontmatter.";
        let rule = RuleParser::parse_with_name("raw", content).expect("parse failed");
        assert_eq!(rule.name, "raw");
        assert_eq!(rule.description, "");
        assert!(rule.globs.is_empty());
        assert!(!rule.always_apply);
        assert_eq!(
            rule.content,
            "Just raw markdown content without frontmatter."
        );
    }

    #[test]
    fn test_rule_parser_unclosed_frontmatter_fails_cleanly() {
        let content = "---\ndescription: Broken\nUnclosed frontmatter";
        let res = RuleParser::parse(content);
        assert!(res.is_err());
        match res.err().unwrap() {
            RulesError::ParseError(msg) => {
                assert!(msg.contains("Unclosed YAML frontmatter"));
            }
            other => panic!("Unexpected error: {:?}", other),
        }
    }

    #[test]
    fn test_rule_parser_legacy_cursorrules() {
        let content = "You are an expert AI assistant.\nAlways check types.";
        let rule = RuleParser::parse_legacy_cursorrules(content).expect("parse failed");
        assert_eq!(rule.name, ".cursorrules");
        assert!(rule.always_apply);
        assert_eq!(
            rule.content,
            "You are an expert AI assistant.\nAlways check types."
        );
    }

    #[test]
    fn test_rule_matcher_globs_and_always_apply() {
        let r1 = Rule::new(
            "frontend",
            "Frontend rules",
            vec!["*.ts".into(), "src/**/*.tsx".into()],
            false,
            "Frontend only",
        );
        let r2 = Rule::new(
            "backend",
            "Rust rules",
            vec!["*.rs".into(), "crates/**/*.rs".into()],
            false,
            "Rust only",
        );
        let r3 = Rule::new("global", "Universal safety", vec![], true, "Always applied");

        let matcher = RuleMatcher::new(vec![r1, r2, r3]).expect("matcher build failed");

        // TypeScript file matches frontend + global
        let matches_ts = matcher.match_path("src/components/Header.tsx");
        let names_ts: Vec<&str> = matches_ts.iter().map(|r| r.name.as_str()).collect();
        assert_eq!(names_ts, vec!["frontend", "global"]);

        // Rust file matches backend + global
        let matches_rs = matcher.match_path("crates/orbit-rules/src/lib.rs");
        let names_rs: Vec<&str> = matches_rs.iter().map(|r| r.name.as_str()).collect();
        assert_eq!(names_rs, vec!["backend", "global"]);

        // Windows backslash path
        let matches_win = matcher.match_path("crates\\orbit-rules\\src\\lib.rs");
        let names_win: Vec<&str> = matches_win.iter().map(|r| r.name.as_str()).collect();
        assert_eq!(names_win, vec!["backend", "global"]);

        // Unmatched file matches only global
        let matches_py = matcher.match_path("scripts/deploy.py");
        let names_py: Vec<&str> = matches_py.iter().map(|r| r.name.as_str()).collect();
        assert_eq!(names_py, vec!["global"]);
    }

    #[test]
    fn test_rule_matcher_matches_rule_helper() {
        let r1 = Rule::new("ts", "", vec!["*.ts".into()], false, "body");
        let r2 = Rule::new("always", "", vec![], true, "body");
        let matcher = RuleMatcher::new(vec![r1.clone(), r2.clone()]).unwrap();

        assert!(matcher.matches_rule(&r1, "main.ts"));
        assert!(!matcher.matches_rule(&r1, "main.rs"));
        assert!(matcher.matches_rule(&r2, "anything.txt"));
    }

    #[test]
    fn test_prompt_section_builder_xml_output() {
        let r1 = Rule::new(
            "react-style",
            "React styling guide",
            vec!["*.tsx".into()],
            false,
            "Use Tailwind classes with prefix void-.",
        );
        let r2 = Rule::new(
            "safety",
            "",
            vec![],
            true,
            "Verify all changes before finalizing.",
        );

        let active_rules = vec![&r1, &r2];
        let xml = PromptSectionBuilder::with_rules(active_rules).build();

        assert!(xml.starts_with("<cursor_rules>\n"));
        assert!(xml.ends_with("</cursor_rules>\n"));
        assert!(xml.contains("<rule name=\"react-style\" description=\"React styling guide\">"));
        assert!(xml.contains("Use Tailwind classes with prefix void-."));
        assert!(xml.contains("</rule>"));
        assert!(xml.contains("<rule name=\"safety\">"));
        assert!(xml.contains("Verify all changes before finalizing."));
    }

    #[test]
    fn test_prompt_section_builder_empty() {
        let builder = PromptSectionBuilder::new();
        assert!(builder.is_empty());
        assert_eq!(builder.build(), "");
        assert_eq!(builder.build_option(), None);
    }

    #[test]
    fn test_prompt_section_builder_xml_escaping() {
        let r = Rule::new(
            "x & y <z>",
            "rules for \"quoted\" & 'special'",
            vec![],
            true,
            "Clean code",
        );
        let xml = PromptSectionBuilder::with_rules(vec![&r]).build();
        assert!(xml.contains("name=\"x &amp; y &lt;z&gt;\""));
        assert!(
            xml.contains("description=\"rules for &quot;quoted&quot; &amp; &apos;special&apos;\"")
        );
    }

    #[test]
    fn test_rules_registry_scans_workspace() {
        let temp_dir =
            std::env::temp_dir().join(format!("orbit_rules_test_{}", std::process::id()));
        let cursor_rules_dir = temp_dir.join(".cursor").join("rules");
        let orbit_rules_dir = temp_dir.join(".orbit").join("rules");

        fs::create_dir_all(&cursor_rules_dir).unwrap();
        fs::create_dir_all(&orbit_rules_dir).unwrap();

        // 1. .cursor/rules/a.mdc
        fs::write(
            cursor_rules_dir.join("a.mdc"),
            "---\ndescription: Cursor rule A\nglobs: [\"*.ts\"]\n---\nRule A body",
        )
        .unwrap();

        // 2. .orbit/rules/b.mdc
        fs::write(
            orbit_rules_dir.join("b.mdc"),
            "---\ndescription: Orbit rule B\nalwaysApply: true\n---\nRule B body",
        )
        .unwrap();

        // 3. .cursorrules
        fs::write(
            temp_dir.join(".cursorrules"),
            "Legacy cursorrules body content",
        )
        .unwrap();

        let registry = RulesRegistry::scan_workspace(&temp_dir).expect("scan failed");
        assert_eq!(registry.len(), 3);

        let rule_a = registry.get_rule("a").expect("rule a not found");
        assert_eq!(rule_a.description, "Cursor rule A");
        assert_eq!(rule_a.globs, vec!["*.ts"]);

        let rule_b = registry.get_rule("b").expect("rule b not found");
        assert!(rule_b.always_apply);

        let legacy = registry
            .get_rule(".cursorrules")
            .expect("legacy rule not found");
        assert!(legacy.always_apply);
        assert_eq!(legacy.content, "Legacy cursorrules body content");

        // Clean up
        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_orbit_rules_override_cursor_rules() {
        let temp_dir =
            std::env::temp_dir().join(format!("orbit_override_test_{}", std::process::id()));
        let cursor_rules_dir = temp_dir.join(".cursor").join("rules");
        let orbit_rules_dir = temp_dir.join(".orbit").join("rules");

        fs::create_dir_all(&cursor_rules_dir).unwrap();
        fs::create_dir_all(&orbit_rules_dir).unwrap();

        // Rule with same name in both
        fs::write(
            cursor_rules_dir.join("shared.mdc"),
            "---\ndescription: Base\n---\nBase content",
        )
        .unwrap();

        fs::write(
            orbit_rules_dir.join("shared.mdc"),
            "---\ndescription: Overridden by Orbit\n---\nOrbit content",
        )
        .unwrap();

        let registry = RulesRegistry::scan_workspace(&temp_dir).expect("scan failed");
        assert_eq!(registry.len(), 1);
        let rule = registry.get_rule("shared").unwrap();
        assert_eq!(rule.description, "Overridden by Orbit");
        assert_eq!(rule.content, "Orbit content");

        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_real_workspace_scan() {
        // Test against the actual workspace root of orbiteditor
        let repo_root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(|p| p.parent())
            .and_then(|p| p.parent())
            .expect("repo root");

        let registry = RulesRegistry::scan_workspace(repo_root).expect("scan repo failed");
        // We know .cursor/rules/orbit-agent-safety-and-ui-consistency.mdc exists
        let safety_rule = registry.get_rule("orbit-agent-safety-and-ui-consistency");
        assert!(safety_rule.is_some());
        let safety = safety_rule.unwrap();
        assert!(safety.always_apply);
        assert!(safety
            .content
            .contains("Orbit Agent Safety and UI Consistency"));
    }
}
