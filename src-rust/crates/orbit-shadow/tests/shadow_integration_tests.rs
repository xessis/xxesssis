use orbit_shadow::{validate_brackets, ShadowBuffer, SpeculativeEditSession};

#[test]
fn test_integration_full_speculative_workflow() {
    let mut session = SpeculativeEditSession::new("speculative-agent-run");

    let initial_main = r#"fn main() {
    println!("hello world");
}
"#;

    let initial_lib = r#"pub fn add(a: i32, b: i32) -> i32 {
    a + b
}
"#;

    session.open_buffer("src/main.rs", initial_main);
    session.open_buffer("src/lib.rs", initial_lib);

    assert_eq!(session.buffer_count(), 2);
    assert!(!session.is_dirty());

    session.begin_transaction().expect("begin transaction");

    // Apply fast apply edit to main.rs
    session
        .apply_search_replace(
            "src/main.rs",
            "    println!(\"hello world\");",
            "    println!(\"hello orbit shadow engine\");",
        )
        .expect("edit main.rs");

    // Apply fast apply edit to lib.rs
    session
        .apply_search_replace(
            "src/lib.rs",
            "pub fn add(a: i32, b: i32) -> i32 {\n    a + b\n}",
            "pub fn add(a: i32, b: i32) -> i32 {\n    // compute sum\n    a + b\n}",
        )
        .expect("edit lib.rs");

    assert!(session.is_dirty());

    // Check preview diffs
    let diffs = session.compute_all_diffs();
    assert_eq!(diffs.len(), 2);

    let main_diff = diffs.get("src/main.rs").expect("main diff exists");
    assert_eq!(main_diff.summary.additions, 1);
    assert_eq!(main_diff.summary.deletions, 1);

    let summary = session.commit_transaction().expect("commit transaction");
    assert_eq!(summary.files.len(), 2);
    assert_eq!(summary.total_summary.additions, 2);
    assert_eq!(summary.total_summary.deletions, 1);

    // Buffers now committed as baseline
    assert!(!session.is_dirty());
    assert_eq!(
        session
            .get_buffer("src/main.rs")
            .unwrap()
            .original_content(),
        "fn main() {\n    println!(\"hello orbit shadow engine\");\n}\n"
    );
}

#[test]
fn test_integration_multi_file_atomic_rollback_on_failure() {
    let mut session = SpeculativeEditSession::new("atomic-rollback-test");

    session.open_buffer("service1.rs", "pub fn s1() -> i32 { 1 }\n");
    session.open_buffer("service2.rs", "pub fn s2() -> i32 { 2 }\n");
    session.open_buffer("service3.rs", "pub fn s3() -> i32 { 3 }\n");

    let result = session.run_transaction(|sess| {
        // Edit 1: succeeds
        sess.apply_rewrite("service1.rs", "pub fn s1() -> i32 { 10 }\n")?;

        // Edit 2: succeeds
        sess.apply_rewrite("service2.rs", "pub fn s2() -> i32 { 20 }\n")?;

        // Edit 3: fails due to search block mismatch
        sess.apply_search_replace("service3.rs", "nonexistent code", "replacement")?;

        Ok(())
    });

    assert!(result.is_err());

    // Crucial check: All 3 buffers must be completely intact and un-mutated!
    assert_eq!(
        session.get_buffer("service1.rs").unwrap().current_content(),
        "pub fn s1() -> i32 { 1 }\n"
    );
    assert_eq!(
        session.get_buffer("service2.rs").unwrap().current_content(),
        "pub fn s2() -> i32 { 2 }\n"
    );
    assert_eq!(
        session.get_buffer("service3.rs").unwrap().current_content(),
        "pub fn s3() -> i32 { 3 }\n"
    );
    assert!(!session.is_dirty());
}

#[test]
fn test_integration_syntax_break_reversion_across_transaction() {
    let mut session = SpeculativeEditSession::new("syntax-guard-test");

    session.open_buffer("routes.ts", "export const routes = [1, 2, 3];\n");
    session.open_buffer("handler.ts", "export function handle() { return true; }\n");

    session.begin_transaction().expect("begin tx");

    // Valid edit to routes.ts
    session
        .apply_rewrite("routes.ts", "export const routes = [1, 2, 3, 4];\n")
        .expect("rewrite routes");

    // Broken edit to handler.ts (missing closing brace)
    session
        .apply_rewrite(
            "handler.ts",
            "export function handle() {\n    if (true) {\n        return false;\n",
        )
        .expect("rewrite handler");

    // Commit should detect bracket imbalance on handler.ts and abort
    let commit_err = session.commit_transaction();
    assert!(commit_err.is_err());

    // routes.ts should have reverted to original content!
    assert_eq!(
        session.get_buffer("routes.ts").unwrap().current_content(),
        "export const routes = [1, 2, 3];\n"
    );
    // handler.ts should have reverted to original content!
    assert_eq!(
        session.get_buffer("handler.ts").unwrap().current_content(),
        "export function handle() { return true; }\n"
    );
}

#[test]
fn test_integration_new_buffer_cleanup_on_rollback() {
    let mut session = SpeculativeEditSession::new("new-buffer-rollback");

    session.open_buffer("existing.rs", "fn existing() {}\n");

    session.begin_transaction().expect("begin tx");

    // Open a brand new buffer inside transaction
    session.open_buffer("temp_created.rs", "fn temp() {}\n");
    assert!(session.has_buffer("temp_created.rs"));

    // Force rollback
    session.rollback_transaction().expect("rollback");

    // Newly added buffer during the rolled-back transaction must be cleaned up
    assert!(!session.has_buffer("temp_created.rs"));
    assert!(session.has_buffer("existing.rs"));
}

#[test]
fn test_integration_conflict_marker_parsing_and_apply() {
    let mut buffer = ShadowBuffer::new(
        "calculator.rs",
        r#"pub fn calculate() -> i32 {
    let a = 1;
    let b = 2;
    a + b
}
"#,
    );

    let patch = r#"
Some explanation from LLM...
<<<<<<< ORIGINAL
    let a = 1;
    let b = 2;
=======
    let a = 10;
    let b = 20;
>>>>>>> UPDATED
"#;

    let applied = buffer
        .apply_search_replace_text(patch)
        .expect("apply conflict format");
    assert_eq!(applied, 1);

    assert!(buffer.current_content().contains("let a = 10;"));
    assert!(buffer.current_content().contains("let b = 20;"));

    let diff = buffer.compute_diff();
    assert!(!diff.hunks.is_empty());
    assert_eq!(diff.summary.additions, 2);
    assert_eq!(diff.summary.deletions, 2);
}

#[test]
fn test_integration_json_serialization_roundtrip() {
    let mut session = SpeculativeEditSession::new("json-session");
    session.open_buffer("config.json", r#"{"key": "initial"}"#);
    session
        .apply_rewrite("config.json", r#"{"key": "updated"}"#)
        .expect("rewrite");

    let serialized = serde_json::to_string(&session).expect("serialize session");
    let deserialized: SpeculativeEditSession =
        serde_json::from_str(&serialized).expect("deserialize session");

    assert_eq!(deserialized.id(), "json-session");
    assert_eq!(deserialized.buffer_count(), 1);
    let buf = deserialized.get_buffer("config.json").unwrap();
    assert_eq!(buf.current_content(), r#"{"key": "updated"}"#);
    assert_eq!(buf.original_content(), r#"{"key": "initial"}"#);
    assert!(buf.is_dirty());
}

#[test]
fn test_bracket_validation_languages() {
    // Rust with attributes, lifetimes, strings, block comments
    let rust_code = r#"
#[derive(Debug, Clone)]
pub struct Worker<'a> {
    pub name: &'a str,
}

impl<'a> Worker<'a> {
    pub fn work(&self) {
        let _s = "bracket { in string";
        /* unclosed [ in comment */
    }
}
"#;
    let report = validate_brackets(rust_code, Some("worker.rs"));
    assert!(report.is_balanced);

    // Python with # and multiline strings
    let python_code = r#"
# Comment with ( [ {
"""
Multiline docstring with unclosed [ ( {
"""
def execute(data):
    return [x for x in data if x > 0]
"#;
    let report = validate_brackets(python_code, Some("worker.py"));
    assert!(report.is_balanced);

    // JS/TS with template literals and nested ${}
    let js_code = r#"
function greet(user) {
    return `Hello, ${user.getName({ formal: true })}!`;
}
"#;
    let report = validate_brackets(js_code, Some("worker.ts"));
    assert!(report.is_balanced);
}
