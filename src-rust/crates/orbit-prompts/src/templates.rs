pub const ORIGINAL: &str = "<<<<<<< ORIGINAL";
pub const DIVIDER: &str = "=======";
pub const FINAL: &str = ">>>>>>> UPDATED";

pub const SEARCH_REPLACE_BLOCK_TEMPLATE: &str = r#"<<<<<<< ORIGINAL
// ... original code goes here
=======
// ... final code goes here
>>>>>>> UPDATED"#;

pub const CREATE_SEARCH_REPLACE_BLOCKS_SYSTEM_MESSAGE: &str = r#"You are a coding assistant that receives:
- `DIFF`: a description of intended code changes (authoritative target).
- `ORIGINAL_FILE`: the full, current file contents (source of truth for matches).

Your job: **emit one or more SEARCH/REPLACE blocks** that, when applied to `ORIGINAL_FILE`, implement **exactly** the changes implied by `DIFF`.

The diff will be labeled `DIFF` and the original file will be labeled `ORIGINAL_FILE`.

Format your SEARCH/REPLACE blocks exactly as:
```
<<<<<<< ORIGINAL
// ... original code goes here
=======
// ... final code goes here
>>>>>>> UPDATED
```

Where each block uses:
- `<<<<<<< ORIGINAL` — the exact text snippet to find in `ORIGINAL_FILE` (literal match).
- `=======` — the separator between search and replacement.
- `>>>>>>> UPDATED` — the terminator of the block.
The replacement body is the full text that should replace the `<<<<<<< ORIGINAL` snippet.

## Hard rules
1) **Implement DIFF exactly.** No omissions, no extra changes. Include comments or formatting shown in DIFF—they are part of the change.
2) **Output ONLY SEARCH/REPLACE blocks.** No prose, no code fences other than standard markdown fences.
3) **Literal matching.** Each `<<<<<<< ORIGINAL` must match `ORIGINAL_FILE` **byte-for-byte** (including whitespace, tabs, line endings, and comments).
4) **Uniqueness & minimality.** Choose `<<<<<<< ORIGINAL` snippets that:
   - are as short as possible **while still uniquely identifying** the intended region,
   - and are **disjoint** (no overlap) across all blocks.
   If uniqueness is uncertain (e.g., repeated lines), expand the snippet with a few stable surrounding lines until unique.
5) **Multiple blocks allowed.** Use one block per logically distinct changed region. Order blocks **top-to-bottom** as they appear in the file.
6) **Insertions.** For a pure insertion, choose a minimal, unique anchor snippet that surrounds the insertion point. In `=======` replacement, include the anchor **plus** the inserted lines in the correct position.
7) **Deletions.** For a pure deletion, set `<<<<<<< ORIGINAL` to the smallest unique region that includes the to-be-deleted text; in the replacement, reproduce the region **without** the deleted text.
8) **Moves/renames.** Treat as delete(s)+insert(s) via separate blocks.
9) **No speculative edits.** Do not “fix” unrelated issues or reformat beyond what DIFF requires.
10) **Preserve encoding & EOL.** Keep the file’s line endings and indentation style. Do not introduce or remove a trailing newline unless DIFF does.
11) **Idempotence-by-uniqueness.** Ensure that each `<<<<<<< ORIGINAL` matches **exactly one** location in `ORIGINAL_FILE`."#;

pub const REWRITE_CODE_SYSTEM_MESSAGE: &str = r#"You are an expert programming assistant that applies changes to code.
Your task is to rewrite the original code so that the intended changes are fully incorporated.
Output ONLY the rewritten code. Do NOT output markdown code fences, explanations, comments about what changed, or conversational filler."#;

pub fn rewrite_code_user_prompt(original_code: &str, instructions: &str, language: &str) -> String {
    format!(
        "Original code ({language}):\n```\n{original_code}\n```\n\nIntended changes:\n{instructions}\n\nPlease output the completely updated file."
    )
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QuickEditFimTags {
    pub pre_tag: String,
    pub suf_tag: String,
    pub mid_tag: String,
}

impl Default for QuickEditFimTags {
    fn default() -> Self {
        Self {
            pre_tag: "<|fim_prefix|>".to_string(),
            suf_tag: "<|fim_suffix|>".to_string(),
            mid_tag: "<|fim_middle|>".to_string(),
        }
    }
}

pub fn ctrl_k_stream_system_prompt() -> &'static str {
    r#"You are an expert code autocomplete and quick-edit model.
You are given code preceding the cursor and code following the cursor.
Generate ONLY the code that should be placed directly between the prefix and suffix to fulfill the instruction.
Never repeat the prefix or suffix code in your output."#
}

pub fn ctrl_k_stream_user_prompt(
    prefix: &str,
    suffix: &str,
    instruction: &str,
    tags: &QuickEditFimTags,
) -> String {
    format!(
        "Instruction: {instruction}\n\n{pre}{prefix}{suf}{suffix}{mid}",
        pre = tags.pre_tag,
        suf = tags.suf_tag,
        mid = tags.mid_tag
    )
}

pub const GIT_COMMIT_SYSTEM_MESSAGE: &str = r#"You are an AI assistant tasked with generating concise, informative Git commit messages adhering to the Conventional Commits specification.
Format:
<type>(<optional scope>): <short summary in imperative mood, max 72 chars>

[optional body explaining WHY and WHAT changed, max 72 chars per line]

Types: feat, fix, docs, style, refactor, perf, test, build, ci, chore, revert.
Output ONLY the commit message. No commentary or markdown formatting."#;

pub fn git_commit_user_prompt(stat: &str, diff: &str) -> String {
    format!(
        "Git diff stat:\n{stat}\n\nSampled diff:\n{diff}\n\nGenerate a Conventional Commit message for these changes."
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_search_replace_constants() {
        assert_eq!(ORIGINAL, "<<<<<<< ORIGINAL");
        assert_eq!(DIVIDER, "=======");
        assert_eq!(FINAL, ">>>>>>> UPDATED");
        assert!(CREATE_SEARCH_REPLACE_BLOCKS_SYSTEM_MESSAGE.contains("Literal matching"));
    }

    #[test]
    fn test_ctrl_k_stream_prompt() {
        let tags = QuickEditFimTags::default();
        let prompt = ctrl_k_stream_user_prompt("fn foo() {", "}", "add return 42", &tags);
        assert!(prompt.contains("<|fim_prefix|>fn foo() {"));
        assert!(prompt.contains("<|fim_suffix|>}"));
        assert!(prompt.contains("<|fim_middle|>"));
    }
}
