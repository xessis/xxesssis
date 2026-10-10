use grep_regex::RegexMatcherBuilder;
use grep_searcher::sinks::UTF8;
use grep_searcher::SearcherBuilder;
use ignore::WalkBuilder;
use orbit_common::{OrbitError, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GrepMatch {
    pub file_path: PathBuf,
    pub line_number: u64,
    pub line_content: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GrepResult {
    pub matches: Vec<GrepMatch>,
    pub total_matches: usize,
    pub files_searched: usize,
}

pub fn search_workspace(
    root: impl AsRef<Path>,
    pattern: &str,
    case_sensitive: bool,
    max_results: usize,
) -> Result<GrepResult> {
    let mut builder = RegexMatcherBuilder::new();
    builder.case_insensitive(!case_sensitive);
    builder.line_terminator(Some(b'\n'));

    let matcher = builder.build(pattern).map_err(|e| {
        OrbitError::ToolExecution(format!("Invalid regex pattern '{}': {}", pattern, e))
    })?;

    if max_results == 0 {
        return Ok(GrepResult {
            matches: Vec::new(),
            total_matches: 0,
            files_searched: 0,
        });
    }

    let mut matches = Vec::new();
    let mut files_searched = 0;

    let walker = WalkBuilder::new(root).hidden(true).parents(true).build();

    let mut searcher = SearcherBuilder::new()
        .binary_detection(grep_searcher::BinaryDetection::quit(0))
        .line_number(true)
        .build();

    for result in walker {
        let entry = match result {
            Ok(e) => e,
            Err(_) => continue,
        };

        if entry.file_type().is_some_and(|ft| ft.is_file()) {
            files_searched += 1;
            let file_path = entry.path().to_path_buf();

            let _ = searcher.search_path(
                &matcher,
                &file_path,
                UTF8(|line_num, line| {
                    if matches.len() >= max_results {
                        return Ok(false);
                    }
                    let trimmed = line.trim_end_matches(&['\r', '\n'][..]);
                    let line_content = if trimmed.len() > 4000 {
                        let mut end = 4000;
                        while !trimmed.is_char_boundary(end) {
                            end -= 1;
                        }
                        format!("{}... [truncated]", &trimmed[..end])
                    } else {
                        trimmed.to_string()
                    };
                    matches.push(GrepMatch {
                        file_path: file_path.clone(),
                        line_number: line_num,
                        line_content,
                    });
                    Ok(matches.len() < max_results)
                }),
            );

            if matches.len() >= max_results {
                break;
            }
        }
    }

    let total = matches.len();
    Ok(GrepResult {
        matches,
        total_matches: total,
        files_searched,
    })
}
