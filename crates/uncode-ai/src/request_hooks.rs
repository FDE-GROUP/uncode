//! LLM 请求观测钩子（对齐 Pi `StreamOptions.on_payload` / `on_response`）。

use std::collections::HashMap;

use reqwest::header::HeaderMap;
use serde_json::Value;

use crate::api_types::StreamOptions;

/// 合并 `StreamOptions.headers` 到 reqwest 请求。
pub fn apply_option_headers(
    mut req: reqwest::RequestBuilder,
    options: &StreamOptions,
) -> reqwest::RequestBuilder {
    if let Some(ref headers) = options.headers {
        for (k, v) in headers {
            req = req.header(k.as_str(), v.as_str());
        }
    }
    req
}

/// 在 HTTP 发送前调用，传入即将提交的 JSON 请求体。
pub fn notify_request_payload(options: &StreamOptions, body: &Value) {
    if let Some(ref cb) = options.on_payload {
        cb(body);
    }
}

/// 在收到 HTTP 响应头后调用（成功或失败路径均可观测状态码）。
pub fn notify_http_response(options: &StreamOptions, status: u16, headers: &HeaderMap) {
    if let Some(ref cb) = options.on_response {
        let map: HashMap<String, String> = headers
            .iter()
            .filter_map(|(k, v)| {
                v.to_str()
                    .ok()
                    .map(|s| (k.as_str().to_string(), s.to_string()))
            })
            .collect();
        cb(status, &map);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[test]
    fn notify_payload_invokes_callback() {
        let count = Arc::new(AtomicUsize::new(0));
        let count2 = count.clone();
        let options = StreamOptions {
            on_payload: Some(Arc::new(move |_v| {
                count2.fetch_add(1, Ordering::SeqCst);
            })),
            ..Default::default()
        };
        notify_request_payload(&options, &serde_json::json!({"model": "x"}));
        assert_eq!(count.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn notify_response_skips_when_unset() {
        let options = StreamOptions::default();
        notify_http_response(&options, 200, &HeaderMap::new());
    }
}
