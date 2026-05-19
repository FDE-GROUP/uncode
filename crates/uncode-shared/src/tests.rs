use crate::config::{AppConfig, ProviderConfigs, WorkspaceGraphConfig};
use crate::error::{
    BranchSummaryError, CompactionError, ExecutionError, FileError, HarnessError, UncodeError,
};

// ── Config tests ──

#[test]
fn test_app_config_default() {
    let config = AppConfig::default();
    assert_eq!(config.model, "deepseek-v3");
    assert_eq!(config.max_tokens, 8192);
    assert_eq!(config.temperature, 0.7);
}

#[test]
fn test_app_config_serialization() {
    let config = AppConfig::default();
    let json = serde_json::to_string(&config).unwrap();
    let parsed: AppConfig = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed.model, config.model);
    assert_eq!(parsed.max_tokens, config.max_tokens);
}

#[test]
fn test_default_models_count() {
    let config = AppConfig::default();
    assert_eq!(config.models.len(), 3);
    assert_eq!(config.models[0].id, "deepseek-v3");
}

#[test]
fn test_workspace_graph_config_default() {
    let wg = WorkspaceGraphConfig::default();
    assert!(wg.enabled);
    assert_eq!(wg.ttl_secs, 21600);
    assert_eq!(wg.max_items, 16);
    assert_eq!(wg.max_bytes, 16384);
    assert_eq!(wg.max_file_bytes, 100_000);
}

#[test]
fn test_provider_configs_default() {
    let pc = ProviderConfigs::default();
    assert!(pc.deepseek.is_none());
    assert!(pc.ollama.is_none());
    assert!(pc.openai.is_none());
}

#[test]
fn test_user_model_config_serialization() {
    let json = r#"{
        "id": "my-model",
        "provider": "openai",
        "base_url": "https://example.com/v1",
        "api_key": "sk-test"
    }"#;
    let _config: crate::config::UserModelConfig = serde_json::from_str(json).unwrap();
}

// ── Error tests ──

#[test]
fn test_file_error_codes() {
    let err = FileError::not_found("test.txt");
    assert_eq!(err.code(), 1001);
    assert!(err.to_string().contains("test.txt"));

    let err = FileError::permission_denied("/root");
    assert_eq!(err.code(), 1002);

    let err = FileError::sandbox_violation("../outside");
    assert_eq!(err.code(), 1003);

    let err = FileError::too_large("big.bin", 1024);
    assert_eq!(err.code(), 1004);
    assert!(err.to_string().contains("1024"));
}

#[test]
fn test_execution_error_codes() {
    let err = ExecutionError::non_zero_exit("ls", 1);
    assert_eq!(err.code(), 2001);

    let err = ExecutionError::timeout("sleep", 5000);
    assert_eq!(err.code(), 2002);

    let err = ExecutionError::cancelled("make");
    assert_eq!(err.code(), 2003);
}

#[test]
fn test_compaction_error_codes() {
    let err = CompactionError::llm_failed("timeout");
    assert_eq!(err.code(), 3001);

    let err = CompactionError::cut_point_not_found();
    assert_eq!(err.code(), 3002);
}

#[test]
fn test_harness_error_codes() {
    let err = HarnessError::busy("running");
    assert_eq!(err.code(), 5001);
    assert!(err.to_string().contains("running"));

    let err = HarnessError::no_session();
    assert_eq!(err.code(), 5002);
}

#[test]
fn test_uncode_error_from_io() {
    let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "no such file");
    let uncode_err: UncodeError = io_err.into();
    match uncode_err {
        UncodeError::File(fe) => assert_eq!(fe.code(), 1001),
        _ => panic!("expected File variant"),
    }
}

#[test]
fn test_uncode_error_tool_variants() {
    let err = UncodeError::Tool("bad args".into());
    assert!(err.to_string().contains("bad args"));

    let err = UncodeError::ToolNotFound {
        name: "ghost".into(),
    };
    assert!(err.to_string().contains("ghost"));
}

#[test]
fn test_uncode_error_llm_variants() {
    let err = UncodeError::Llm("api error".into());
    assert!(err.to_string().contains("LLM error"));

    let err = UncodeError::LlmAuth("invalid key".into());
    assert!(err.to_string().contains("authentication"));

    let err = UncodeError::LlmRateLimit("too many".into());
    assert!(err.to_string().contains("rate"));
}

#[test]
fn test_branch_summary_error() {
    let err = BranchSummaryError::LlmFailed {
        message: "timeout".into(),
        code: 4001,
    };
    assert!(err.to_string().contains("timeout"));
    assert_eq!(err.code(), 4001);

    let err = BranchSummaryError::TargetNotFound {
        target_id: "abc".into(),
        code: 4002,
    };
    assert!(err.to_string().contains("abc"));
    assert_eq!(err.code(), 4002);
}
