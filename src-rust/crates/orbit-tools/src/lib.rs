pub mod file_ops;
pub mod glob;
pub mod search;
pub mod security;

pub use file_ops::{atomic_write_file, read_file_lines, FileReadOutput};
pub use glob::glob_files;
pub use search::{search_workspace, GrepMatch, GrepResult};
pub use security::{assert_no_traversal, is_inside_workspace, is_sensitive_path};

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn test_security_traversal_detection() {
        assert!(assert_no_traversal("src/main.rs").is_ok());
        assert!(assert_no_traversal("../secret.txt").is_err());
        assert!(assert_no_traversal("a/b/../../c").is_err());
    }

    #[test]
    fn test_sensitive_path_detection() {
        assert!(is_sensitive_path(".env"));
        assert!(is_sensitive_path(".env.local"));
        assert!(is_sensitive_path("~/.ssh/id_rsa"));
        assert!(is_sensitive_path("config/aws_credentials"));
        assert!(!is_sensitive_path("src/components/Sidebar.tsx"));
    }

    #[test]
    fn test_atomic_file_write_and_read() {
        let temp_dir = std::env::temp_dir().join(format!("orbit_test_{}", uuid::Uuid::new_v4()));
        let file_path = temp_dir.join("test_write.txt");

        let content = "line 1\nline 2\nline 3\nline 4\n";
        atomic_write_file(&file_path, content).expect("write failed");

        let read_output = read_file_lines(&file_path, 2, 2).expect("read failed");
        assert_eq!(read_output.total_lines, 4);
        assert_eq!(read_output.line_count_returned, 2);
        assert!(read_output.content.contains("line 2"));
        assert!(read_output.content.contains("line 3"));

        let _ = fs::remove_dir_all(temp_dir);
    }
}
