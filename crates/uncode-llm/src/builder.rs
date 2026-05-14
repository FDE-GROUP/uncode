use crate::driver::CompletionRequest;
use uncode_core::message::Message;
use uncode_core::tool::ToolDefinition;

pub struct CompletionRequestBuilder {
    model: String,
    messages: Vec<Message>,
    system: Option<String>,
    max_tokens: Option<u32>,
    temperature: Option<f32>,
    tools: Vec<ToolDefinition>,
}

impl CompletionRequestBuilder {
    pub fn new(model: impl Into<String>) -> Self {
        Self {
            model: model.into(),
            messages: Vec::new(),
            system: None,
            max_tokens: Some(8192),
            temperature: Some(0.7),
            tools: Vec::new(),
        }
    }

    pub fn messages(mut self, messages: Vec<Message>) -> Self {
        self.messages = messages;
        self
    }

    pub fn system(mut self, system: impl Into<String>) -> Self {
        self.system = Some(system.into());
        self
    }

    pub fn max_tokens(mut self, max_tokens: u32) -> Self {
        self.max_tokens = Some(max_tokens);
        self
    }

    pub fn temperature(mut self, temperature: f32) -> Self {
        self.temperature = Some(temperature);
        self
    }

    pub fn tools(mut self, tools: Vec<ToolDefinition>) -> Self {
        self.tools = tools;
        self
    }

    pub fn build(self) -> CompletionRequest {
        CompletionRequest {
            model: self.model,
            messages: self.messages,
            system: self.system,
            max_tokens: self.max_tokens,
            temperature: self.temperature,
            tools: self.tools,
        }
    }
}
