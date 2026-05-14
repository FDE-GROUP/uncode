use async_trait::async_trait;
use futures::stream::{self, BoxStream};
use uncode_core::error::UncodeResult;

use crate::driver::{CompletionRequest, LlmDriver, StreamEvent};

pub struct AnthropicDriver {
    api_key: String,
}

impl AnthropicDriver {
    pub fn new(api_key: String) -> Self {
        Self { api_key }
    }
}

#[async_trait]
impl LlmDriver for AnthropicDriver {
    async fn complete(
        &self,
        _request: CompletionRequest,
    ) -> UncodeResult<BoxStream<'static, StreamEvent>> {
        Ok(Box::pin(stream::once(async { StreamEvent::Done })))
    }

    fn provider_name(&self) -> &'static str {
        "anthropic"
    }
}
