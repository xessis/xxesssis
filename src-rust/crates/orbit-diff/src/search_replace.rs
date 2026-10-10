use orbit_common::{OrbitError, Result};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SearchReplaceBlock {
    pub original: String,
    pub replacement: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MatchStrategy {
    Exact,
    TrimmedLine,
}

pub fn parse_search_replace_blocks(text: &str) -> Vec<SearchReplaceBlock> {
    let mut blocks = Vec::new();
    let lines: Vec<&str> = text.lines().collect();
    let mut i = 0;

    while i < lines.len() {
        let line = lines[i].trim();
        if line.starts_with("<<<<<<<") {
            let mut original_lines = Vec::new();
            let mut replacement_lines = Vec::new();
            let mut in_original = true;
            let mut in_replacement = false;
            let mut found_end = false;
            i += 1;

            while i < lines.len() {
                let inner = lines[i];
                let inner_trim = inner.trim();
                if inner_trim.starts_with("=======") {
                    in_original = false;
                    in_replacement = true;
                    i += 1;
                    continue;
                } else if inner_trim.starts_with(">>>>>>>") {
                    found_end = true;
                    blocks.push(SearchReplaceBlock {
                        original: original_lines.join("\n"),
                        replacement: replacement_lines.join("\n"),
                    });
                    break;
                }

                if in_original {
                    original_lines.push(inner);
                } else if in_replacement {
                    replacement_lines.push(inner);
                }
                i += 1;
            }

            if !found_end && in_replacement {
                blocks.push(SearchReplaceBlock {
                    original: original_lines.join("\n"),
                    replacement: replacement_lines.join("\n"),
                });
            }
        }
        i += 1;
    }

    blocks
}

fn is_line_anchored(haystack: &str, at: usize, len: usize) -> bool {
    let bytes = haystack.as_bytes();
    let starts_at_line = at == 0 || bytes[at - 1] == b'\n';
    let end_idx = at + len;
    let ends_at_line = end_idx == bytes.len()
        || bytes[end_idx] == b'\n'
        || (bytes[end_idx] == b'\r'
            && (end_idx + 1 == bytes.len() || bytes.get(end_idx + 1) == Some(&b'\n')));
    starts_at_line && ends_at_line
}

/// Applies a single search/replace block onto `source`.
/// Strategy:
/// 1. Line-anchored exact match. If found exactly once, replace whole lines.
/// 2. If multiple exact matches, fail (ambiguous).
/// 3. Fallback: line-by-line whitespace-trimmed match.
pub fn apply_block(source: &str, block: &SearchReplaceBlock) -> Result<String> {
    let original = block.original.as_str();
    let replacement = block.replacement.as_str();

    if original.trim().is_empty() {
        return Err(OrbitError::DiffApply(
            "Empty search block cannot be applied".to_string(),
        ));
    }

    // 1. Line-anchored exact match
    let line_anchored_matches: Vec<usize> = source
        .match_indices(original)
        .filter_map(|(idx, _)| {
            if is_line_anchored(source, idx, original.len()) {
                Some(idx)
            } else {
                None
            }
        })
        .collect();

    if line_anchored_matches.len() == 1 {
        let idx = line_anchored_matches[0];
        let mut result = String::with_capacity(source.len() + replacement.len());
        result.push_str(&source[..idx]);
        result.push_str(replacement);
        result.push_str(&source[idx + original.len()..]);
        return Ok(result);
    } else if line_anchored_matches.len() > 1 {
        return Err(OrbitError::DiffApply(
            "Search block is not unique: multiple exact matches found in document".to_string(),
        ));
    }

    // 2. Whitespace-trimmed line fallback
    apply_trimmed_fallback(source, original, replacement)
}

fn apply_trimmed_fallback(source: &str, original: &str, replacement: &str) -> Result<String> {
    let src_lines: Vec<&str> = source.lines().collect();
    let orig_lines: Vec<&str> = original.lines().collect();

    if orig_lines.is_empty() {
        return Err(OrbitError::DiffApply("Empty search block".to_string()));
    }

    if orig_lines.len() > src_lines.len() {
        return Err(OrbitError::DiffApply(
            "Search block has more lines than document".to_string(),
        ));
    }

    let orig_trimmed: Vec<&str> = orig_lines.iter().map(|l| l.trim()).collect();
    let mut candidate_starts = Vec::new();

    let max_start = src_lines.len() - orig_lines.len();
    for i in 0..=max_start {
        let window = &src_lines[i..i + orig_lines.len()];
        let matches = window
            .iter()
            .zip(&orig_trimmed)
            .all(|(s, o)| s.trim() == *o);

        if matches {
            candidate_starts.push(i);
        }
    }

    if candidate_starts.is_empty() {
        return Err(OrbitError::DiffApply(
            "Search block not found in document".to_string(),
        ));
    }

    if candidate_starts.len() > 1 {
        return Err(OrbitError::DiffApply(
            "Search block matches multiple locations with trimmed whitespace".to_string(),
        ));
    }

    let is_crlf = source.contains("\r\n");
    let newline = if is_crlf { "\r\n" } else { "\n" };

    let start_idx = candidate_starts[0];
    let mut new_lines = Vec::new();
    new_lines.extend_from_slice(&src_lines[..start_idx]);
    new_lines.extend(replacement.lines());
    new_lines.extend_from_slice(&src_lines[start_idx + orig_lines.len()..]);

    let mut result = new_lines.join(newline);
    if (is_crlf && source.ends_with("\r\n") && !result.ends_with("\r\n"))
        || (!is_crlf && source.ends_with('\n') && !result.ends_with('\n'))
    {
        result.push_str(newline);
    }
    Ok(result)
}

pub fn apply_all_blocks(source: &str, blocks: &[SearchReplaceBlock]) -> Result<String> {
    let mut current = source.to_string();
    for (idx, block) in blocks.iter().enumerate() {
        match apply_block(&current, block) {
            Ok(updated) => current = updated,
            Err(e) => {
                return Err(OrbitError::DiffApply(format!(
                    "Block {} failed to apply: {}",
                    idx + 1,
                    e
                )))
            }
        }
    }
    Ok(current)
}
