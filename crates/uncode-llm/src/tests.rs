#[cfg(test)]
mod tests {
    use crate::builder::CompletionRequestBuilder;
    use uncode_core::message::Message;

    #[test]
    fn test_builder_minimal() {
        let req = CompletionRequestBuilder::new("deepseek-v3")
            .messages(vec![Message::user("hi")])
            .build();
        assert_eq!(req.model, "deepseek-v3");
        assert_eq!(req.messages.len(), 1);
        assert_eq!(req.max_tokens, Some(8192));
        assert_eq!(req.temperature, Some(0.7));
    }

    #[test]
    fn test_builder_full() {
        let req = CompletionRequestBuilder::new("gpt-4")
            .messages(vec![Message::user("hello")])
            .system("you are helpful")
            .max_tokens(1024)
            .temperature(0.3)
            .build();
        assert_eq!(req.model, "gpt-4");
        assert_eq!(req.system, Some("you are helpful".into()));
        assert_eq!(req.max_tokens, Some(1024));
        assert_eq!(req.temperature, Some(0.3));
    }

    #[test]
    fn test_builder_with_tools() {
        let tools = vec![uncode_core::tool::ToolDefinition {
            name: "read".into(),
            description: "read file".into(),
            parameters: serde_json::json!({}),
        }];
        let req = CompletionRequestBuilder::new("claude")
            .messages(vec![Message::user("test")])
            .tools(tools)
            .build();
        assert_eq!(req.tools.len(), 1);
        assert_eq!(req.tools[0].name, "read");
    }
}
