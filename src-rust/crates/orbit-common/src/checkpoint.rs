use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileSnapshot {
    pub path: String,
    pub content: String,
    pub hash: String,
}

impl FileSnapshot {
    pub fn new(path: impl AsRef<Path>, content: String) -> Self {
        let hash = blake3::hash(content.as_bytes()).to_hex().to_string();
        let path_str = path.as_ref().to_string_lossy().replace('\\', "/");
        Self {
            path: path_str,
            content,
            hash,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CheckpointNode {
    pub id: Uuid,
    pub timestamp_ms: i64,
    pub files: HashMap<String, FileSnapshot>,
    pub description: Option<String>,
}

impl CheckpointNode {
    pub fn new(description: Option<String>) -> Self {
        Self {
            id: Uuid::new_v4(),
            timestamp_ms: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_millis() as i64)
                .unwrap_or(0),
            files: HashMap::new(),
            description,
        }
    }

    pub fn record_file(&mut self, path: impl AsRef<Path>, content: String) {
        let snapshot = FileSnapshot::new(path.as_ref(), content);
        self.files.insert(snapshot.path.clone(), snapshot);
    }
}
