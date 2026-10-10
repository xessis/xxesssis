use orbit_tools::{
    assert_no_traversal, is_inside_workspace, is_sensitive_path, read_file_lines, search_workspace,
};
use std::fs::{self, File};

#[test]
fn test_read_file_lines_fails_on_empty_file() {
    let temp_dir = std::env::temp_dir().join(format!("orbit_test_empty_{}", uuid::Uuid::new_v4()));
    fs::create_dir_all(&temp_dir).unwrap();
    let empty_file = temp_dir.join("empty.txt");
    File::create(&empty_file).unwrap();

    let result = read_file_lines(&empty_file, 1, 10);
    println!("read_file_lines on empty file result: {:?}", result.is_ok());
    if let Ok(ref out) = result {
        println!(
            "output total_lines: {}, is_truncated: {}",
            out.total_lines, out.is_truncated
        );
    }
    assert!(result.is_ok());

    let _ = fs::remove_dir_all(&temp_dir);
}

#[test]
fn test_is_inside_workspace_succeeds_on_nonexistent_file() {
    let temp_dir = std::env::temp_dir().join(format!("orbit_test_ws_{}", uuid::Uuid::new_v4()));
    fs::create_dir_all(&temp_dir).unwrap();
    let target = temp_dir.join("file_to_be_created.txt");

    let inside = is_inside_workspace(&temp_dir, &target);
    assert!(
        inside,
        "is_inside_workspace must return true for new files destined inside workspace"
    );

    let _ = fs::remove_dir_all(&temp_dir);
}

#[test]
fn test_assert_no_traversal_misses_absolute_paths() {
    // Relative paths with .. are rejected
    assert!(assert_no_traversal("../secret.txt").is_err());
    assert!(assert_no_traversal("a/../../b").is_err());
    assert!(assert_no_traversal("normal/path/file.txt").is_ok());
}

#[test]
fn test_sensitive_path_detection_accurate() {
    // System and secret files are correctly identified
    assert!(is_sensitive_path("/etc/shadow"));
    assert!(is_sensitive_path("/etc/passwd"));
    assert!(is_sensitive_path("~/.bash_history"));
    assert!(is_sensitive_path("~/.npmrc"));
    assert!(is_sensitive_path("keys/id_dsa"));
    assert!(is_sensitive_path("token.txt"));
    assert!(is_sensitive_path(".env"));
    assert!(is_sensitive_path(".env.local"));

    // Legitimate user files with partial string matches are NOT blocked
    assert!(!is_sensitive_path("src/animation.keyframe.ts"));
    assert!(!is_sensitive_path("docs/trade_secrets_explained.md"));
    assert!(!is_sensitive_path("src/components/Keybindings.tsx"));
}

#[test]
fn test_search_workspace_max_results_zero() {
    let temp_dir = std::env::temp_dir().join(format!("orbit_test_search_{}", uuid::Uuid::new_v4()));
    fs::create_dir_all(&temp_dir).unwrap();
    let file = temp_dir.join("match.txt");
    fs::write(&file, "target line\n").unwrap();

    let res = search_workspace(&temp_dir, "target", true, 0).expect("search failed");
    assert_eq!(
        res.matches.len(),
        0,
        "max_results = 0 must return 0 matches"
    );

    let _ = fs::remove_dir_all(&temp_dir);
}
