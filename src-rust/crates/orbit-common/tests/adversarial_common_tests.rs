use orbit_common::{ChatMessage, FileSnapshot};
use std::path::PathBuf;

#[test]
fn test_file_snapshot_hash_serializes_as_hex_string() {
    let snap = FileSnapshot::new("main.rs", "fn main() {}".to_string());
    let json = serde_json::to_string(&snap).expect("serialize failed");

    // Blake3 hash must serialize as standard 64-char hex string
    assert!(
        json.contains(r#""hash":""#),
        "Blake3 hash must serialize as hex string"
    );
    assert_eq!(snap.hash.len(), 64);
}

#[test]
fn test_tool_result_content_not_duplicated_in_json() {
    let output_data = "x".repeat(10_000);
    let msg = ChatMessage::tool_result("call_abc", output_data.clone(), false);
    let json = serde_json::to_string(&msg).expect("serialize failed");

    // Tool result output is stored once in content, no duplicate 10k payload in tool_result
    assert_eq!(msg.content, output_data);
    assert_eq!(msg.tool_result.as_ref().unwrap().call_id, "call_abc");
    assert!(
        json.len() < 12_000,
        "Tool result output must not be duplicated in JSON serialization"
    );
}

#[test]
fn test_windows_path_serialization_normalized() {
    let win_path = PathBuf::from("src\\components\\Editor.tsx");
    let snap = FileSnapshot::new(&win_path, "code".to_string());
    let json = serde_json::to_string(&snap).expect("serialize failed");

    // Path must be normalized to forward slashes
    assert!(
        json.contains(r#"src/components/Editor.tsx"#),
        "Paths must be normalized to forward slashes"
    );
}
