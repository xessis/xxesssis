use crate::error::{Result, RetrievalError};
use ignore::WalkBuilder;
use serde::{Deserialize, Serialize};
use std::path::Path;

/// A chunk of source code representing a range of lines in a file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CodeChunk {
    /// Unique identifier for this chunk (e.g. "path/to/file.rs:1-60")
    pub id: String,
    /// Path to the source file
    pub file_path: String,
    /// 1-based start line (inclusive)
    pub start_line: usize,
    /// 1-based end line (inclusive)
    pub end_line: usize,
    /// Text content of the chunk
    pub content: String,
    /// Blake3 hex digest of the chunk content
    pub hash: String,
}

impl CodeChunk {
    /// Creates a new code chunk and computes its Blake3 hash.
    pub fn new(
        file_path: impl Into<String>,
        start_line: usize,
        end_line: usize,
        content: impl Into<String>,
    ) -> Self {
        let file_path = file_path.into();
        let content = content.into();
        let hash = blake3::hash(content.as_bytes()).to_hex().to_string();
        let id = format!("{}:{}-{}", file_path, start_line, end_line);
        Self {
            id,
            file_path,
            start_line,
            end_line,
            content,
            hash,
        }
    }

    /// Number of lines in this chunk
    pub fn line_count(&self) -> usize {
        if self.end_line >= self.start_line {
            self.end_line - self.start_line + 1
        } else {
            0
        }
    }
}

/// Configuration options for the code chunker.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChunkerConfig {
    /// Minimum target lines per chunk (e.g. 50)
    pub min_lines: usize,
    /// Maximum target lines per chunk (e.g. 80)
    pub max_lines: usize,
    /// Overlap lines between consecutive chunks (e.g. 10)
    pub overlap_lines: usize,
    /// Max file size in bytes to index (skips larger files)
    pub max_file_bytes: u64,
}

impl Default for ChunkerConfig {
    fn default() -> Self {
        Self {
            min_lines: 50,
            max_lines: 80,
            overlap_lines: 10,
            max_file_bytes: 2 * 1024 * 1024, // 2MB
        }
    }
}

impl ChunkerConfig {
    pub fn new(min_lines: usize, max_lines: usize, overlap_lines: usize) -> Result<Self> {
        if min_lines == 0 || max_lines == 0 {
            return Err(RetrievalError::InvalidConfig(
                "Chunk sizes must be greater than zero".into(),
            ));
        }
        if min_lines > max_lines {
            return Err(RetrievalError::InvalidConfig(
                "min_lines cannot exceed max_lines".into(),
            ));
        }
        if overlap_lines >= min_lines {
            return Err(RetrievalError::InvalidConfig(
                "overlap_lines must be strictly less than min_lines".into(),
            ));
        }
        Ok(Self {
            min_lines,
            max_lines,
            overlap_lines,
            max_file_bytes: 2 * 1024 * 1024,
        })
    }
}

/// Splits files into semantic chunks with line tracking and Blake3 hashing.
#[derive(Debug, Clone, Default)]
pub struct CodeChunker {
    config: ChunkerConfig,
}

impl CodeChunker {
    pub fn new(config: ChunkerConfig) -> Self {
        Self { config }
    }

    pub fn config(&self) -> &ChunkerConfig {
        &self.config
    }

    /// Splits file text into chunks according to config.
    pub fn chunk_text(&self, file_path: &str, content: &str) -> Vec<CodeChunk> {
        if content.trim().is_empty() {
            return Vec::new();
        }

        let lines: Vec<&str> = content.lines().collect();
        let total_lines = lines.len();
        if total_lines == 0 {
            return Vec::new();
        }

        // If file fits within max_lines, yield a single chunk
        if total_lines <= self.config.max_lines {
            let chunk_content = lines.join("\n");
            return vec![CodeChunk::new(file_path, 1, total_lines, chunk_content)];
        }

        let mut chunks = Vec::new();
        let mut start_idx = 0; // 0-based index

        while start_idx < total_lines {
            let remaining = total_lines - start_idx;
            let end_idx = if remaining <= self.config.max_lines {
                // Last segment fits completely
                total_lines
            } else {
                // Find semantic boundary between min_lines and max_lines
                let min_boundary = start_idx + self.config.min_lines;
                let max_boundary = (start_idx + self.config.max_lines).min(total_lines);
                self.find_semantic_boundary(&lines, min_boundary, max_boundary)
            };

            let chunk_lines = &lines[start_idx..end_idx];
            let chunk_content = chunk_lines.join("\n");
            let start_line = start_idx + 1; // 1-based
            let end_line = end_idx; // 1-based inclusive

            chunks.push(CodeChunk::new(
                file_path,
                start_line,
                end_line,
                chunk_content,
            ));

            if end_idx >= total_lines {
                break;
            }

            // Step next start index backward by overlap
            let next_start = end_idx.saturating_sub(self.config.overlap_lines);
            if next_start <= start_idx {
                // Ensure strictly monotonic forward progress
                start_idx += 1;
            } else {
                start_idx = next_start;
            }

            // If remaining lines are too small (only overlap remainder), stop to avoid duplicate tail
            if total_lines - start_idx <= self.config.overlap_lines {
                break;
            }
        }

        chunks
    }

    /// Searches for a clean split boundary in `lines[min_bound..max_bound]`.
    /// Prefers blank lines or top-level declaration boundaries.
    fn find_semantic_boundary(&self, lines: &[&str], min_bound: usize, max_bound: usize) -> usize {
        // Pass 1: Look for blank lines in the target window (break after blank line)
        for i in (min_bound..max_bound).rev() {
            if lines[i].trim().is_empty() {
                return i + 1;
            }
        }

        // Pass 2: Look for top-level declaration lines (break before declaration)
        for i in (min_bound..max_bound).rev() {
            let line = lines[i];
            if is_semantic_declaration_start(line) {
                return i;
            }
        }

        // Pass 3: Look for closing brace at indent 0 (break after brace)
        for i in (min_bound..max_bound).rev() {
            let trimmed = lines[i].trim();
            if trimmed == "}" || trimmed == "};" || trimmed == "})" {
                return i + 1;
            }
        }

        max_bound
    }

    /// Reads and chunks a file from disk.
    pub fn chunk_file(&self, path: impl AsRef<Path>) -> Result<Vec<CodeChunk>> {
        let path_ref = path.as_ref();
        let metadata = std::fs::metadata(path_ref)?;
        if metadata.len() > self.config.max_file_bytes {
            return Ok(Vec::new());
        }

        let content = match std::fs::read_to_string(path_ref) {
            Ok(c) => c,
            Err(_) => {
                // Non-UTF8 or binary file, skip gracefully
                return Ok(Vec::new());
            }
        };

        let file_path = path_ref.to_string_lossy().replace('\\', "/");
        Ok(self.chunk_text(&file_path, &content))
    }

    /// Walks a workspace directory using `ignore` and chunks all matching files.
    pub fn chunk_workspace(&self, root: impl AsRef<Path>) -> Result<Vec<CodeChunk>> {
        let root_path = root.as_ref();
        let mut all_chunks = Vec::new();

        let walker = WalkBuilder::new(root_path)
            .hidden(true)
            .parents(true)
            .git_ignore(true)
            .git_global(true)
            .git_exclude(true)
            .build();

        for entry in walker {
            let entry = match entry {
                Ok(e) => e,
                Err(_) => continue,
            };

            let file_type = match entry.file_type() {
                Some(ft) => ft,
                None => continue,
            };

            if !file_type.is_file() {
                continue;
            }

            let path = entry.path();
            if is_likely_binary_or_ignored_file(path) {
                continue;
            }

            if let Ok(mut chunks) = self.chunk_file(path) {
                all_chunks.append(&mut chunks);
            }
        }

        Ok(all_chunks)
    }
}

/// Checks whether a line looks like the start of a top-level code definition.
fn is_semantic_declaration_start(line: &str) -> bool {
    let trimmed = line.trim_start();
    // Must not be heavily indented (outermost scope or class/mod level)
    let indent = line.len() - trimmed.len();
    if indent > 4 {
        return false;
    }

    trimmed.starts_with("pub fn ")
        || trimmed.starts_with("pub async fn ")
        || trimmed.starts_with("fn ")
        || trimmed.starts_with("async fn ")
        || trimmed.starts_with("pub struct ")
        || trimmed.starts_with("struct ")
        || trimmed.starts_with("pub enum ")
        || trimmed.starts_with("enum ")
        || trimmed.starts_with("pub trait ")
        || trimmed.starts_with("trait ")
        || trimmed.starts_with("impl ")
        || trimmed.starts_with("pub mod ")
        || trimmed.starts_with("mod ")
        || trimmed.starts_with("class ")
        || trimmed.starts_with("export class ")
        || trimmed.starts_with("export function ")
        || trimmed.starts_with("function ")
        || trimmed.starts_with("def ")
        || trimmed.starts_with("export const ")
        || trimmed.starts_with("export default ")
}

/// Skips common binary, lockfile, minified, or media extensions.
fn is_likely_binary_or_ignored_file(path: &Path) -> bool {
    let ext = match path.extension().and_then(|s| s.to_str()) {
        Some(e) => e.to_ascii_lowercase(),
        None => return false,
    };

    matches!(
        ext.as_str(),
        "png"
            | "jpg"
            | "jpeg"
            | "gif"
            | "ico"
            | "webp"
            | "svg"
            | "pdf"
            | "exe"
            | "dll"
            | "so"
            | "dylib"
            | "bin"
            | "zip"
            | "tar"
            | "gz"
            | "7z"
            | "lock"
            | "woff"
            | "woff2"
            | "ttf"
            | "eot"
            | "mp3"
            | "mp4"
            | "wasm"
            | "map"
    )
}
