//! 交互对话框类型 — 平台无关的请求/响应枚举。
//!
//! Extension 通过 `show_dialog()` 发起对话框，TUI / Platform 渲染并返回用户响应。

use serde::{Deserialize, Serialize};

/// 扩展发起的对话框请求。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum DialogRequest {
    /// 单选列表。
    Select { title: String, options: Vec<String> },
    /// 确认对话框。
    Confirm { message: String },
    /// 文本输入。
    Input {
        prompt: String,
        #[serde(default)]
        default: Option<String>,
    },
}

/// 用户对对话框的响应。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum DialogResponse {
    /// Select 返回选中项索引。
    Selected(usize),
    /// Confirm 返回布尔值。
    Confirmed(bool),
    /// Input 返回输入文本。
    Input(String),
    /// 用户取消。
    Cancelled,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dialog_request_roundtrip() {
        let req = DialogRequest::Select {
            title: "Pick one".into(),
            options: vec!["a".into(), "b".into()],
        };
        let json = serde_json::to_string(&req).unwrap();
        let back: DialogRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(req, back);
    }

    #[test]
    fn dialog_response_roundtrip() {
        for resp in [
            DialogResponse::Selected(2),
            DialogResponse::Confirmed(true),
            DialogResponse::Input("hello".into()),
            DialogResponse::Cancelled,
        ] {
            let json = serde_json::to_string(&resp).unwrap();
            let back: DialogResponse = serde_json::from_str(&json).unwrap();
            assert_eq!(resp, back);
        }
    }

    #[test]
    fn dialog_request_confirm() {
        let req = DialogRequest::Confirm {
            message: "Are you sure?".into(),
        };
        let json = serde_json::to_string(&req).unwrap();
        assert!(json.contains("Confirm"));
        let back: DialogRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(req, back);
    }

    #[test]
    fn dialog_request_input_with_default() {
        let req = DialogRequest::Input {
            prompt: "Name".into(),
            default: Some("foo".into()),
        };
        let json = serde_json::to_string(&req).unwrap();
        let back: DialogRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(req, back);
    }

    #[test]
    fn dialog_request_input_without_default() {
        let req = DialogRequest::Input {
            prompt: "Name".into(),
            default: None,
        };
        let json = serde_json::to_string(&req).unwrap();
        let back: DialogRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(req, back);
    }
}
