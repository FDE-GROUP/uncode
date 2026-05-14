use async_trait::async_trait;
use futures::stream::{self, BoxStream};
use uncode_core::error::UncodeResult;

use crate::driver::{CompletionRequest, LlmDriver, StreamEvent};

pub struct OpenAiDriver {
    api_key: String,
    base_url: String,
}

impl OpenAiDriver {
    pub fn new(api_key: String, base_url: Option<String>) -> Self {
        Self {
            api_key,
            base_url: base_url.unwrap_or_else(|| "https://api.openai.com/v1".into()),
        }
    }
}

#[async_trait]
impl LlmDriver for OpenAiDriver {
    async fn complete(
        &self,
        _request: CompletionRequest,
    ) -> UncodeResult<BoxStream<'static, StreamEvent>> {
        Ok(Box::pin(stream::once(async { StreamEvent::Done })))
    }

    fn provider_name(&self) -> &'static str {
        "openai"
    }
}
