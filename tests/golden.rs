use std::process::Command;

#[test]
fn golden_compile_check() {
    let output = Command::new("cargo")
        .args(["build", "--workspace"])
        .output()
        .unwrap();
    assert!(output.status.success(), "workspace build failed");
}

#[test]
fn golden_test_suite_passes() {
    let output = Command::new("cargo")
        .args(["test", "--workspace"])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "test suite failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn golden_cli_help_works() {
    let output = Command::new("cargo")
        .args(["run", "-p", "uncode-cli", "--", "--help"])
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("uncode"), "cli help should mention uncode");
    assert!(stdout.contains("--model"), "should list --model flag");
    assert!(stdout.contains("--issue"), "should list --issue flag");
}

#[test]
fn golden_session_jsonl_format() {
    use uncode_core::session::{MessageEntry, SessionEntry};
    use uncode_core::message::Message;

    let msg = Message::user("test");
    let entry = SessionEntry::Message(MessageEntry::from(msg));
    let json = serde_json::to_string(&entry).unwrap();

    assert!(json.contains(r#""type":"message""#));
    assert!(json.contains(r#""role":"user""#));
    assert!(json.contains("test"));
}
