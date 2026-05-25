//! Dynamic LLM provider registration — extension-facing types.
//!
//! Extensions register providers by specifying a protocol, base URL, and model
//! list. The host bridges these into `uncode-ai`'s ModelRegistry.

/// API protocol that a dynamic provider uses.
#[derive(Debug, Clone, serde::Deserialize)]
pub enum ProviderProtocol {
    #[serde(rename = "openai")]
    OpenAI,
    #[serde(rename = "anthropic")]
    Anthropic,
    #[serde(rename = "gemini")]
    Gemini,
    #[serde(rename = "ollama")]
    Ollama,
}

impl ProviderProtocol {
    /// Map to the API name used by `uncode-ai`'s ApiRegistry.
    pub fn api_name(&self) -> &'static str {
        match self {
            Self::OpenAI => "openai-completions",
            Self::Anthropic => "anthropic-messages",
            Self::Gemini => "google-generative-ai",
            Self::Ollama => "ollama-native",
        }
    }
}

/// A single model exposed by a dynamic provider.
#[derive(Debug, Clone, serde::Deserialize)]
pub struct DynamicModelDescriptor {
    pub id: String,
    pub name: String,
    pub context_window: u32,
    pub max_output_tokens: u32,
}

/// Registration request for a dynamic LLM provider.
#[derive(Debug, Clone, serde::Deserialize)]
pub struct ProviderRegistration {
    /// Unique provider name (used as model provider prefix).
    pub name: String,
    /// API protocol to use.
    pub protocol: ProviderProtocol,
    /// Base URL for the API endpoint.
    pub base_url: String,
    /// Environment variable name holding the API key.
    pub api_key_env: Option<String>,
    /// Models offered by this provider.
    pub models: Vec<DynamicModelDescriptor>,
}

impl ProviderRegistration {
    #[must_use]
    pub fn validate(&self) -> Result<(), String> {
        if self.name.is_empty() {
            return Err("provider name cannot be empty".into());
        }
        if self.base_url.is_empty() {
            return Err("provider base_url cannot be empty".into());
        }
        if self.models.is_empty() {
            return Err(format!(
                "provider '{}' must declare at least one model",
                self.name
            ));
        }
        for m in &self.models {
            if m.id.is_empty() {
                return Err(format!(
                    "provider '{}' has a model with empty id",
                    self.name
                ));
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_registration(
        name: &str,
        base_url: &str,
        models: Vec<DynamicModelDescriptor>,
    ) -> ProviderRegistration {
        ProviderRegistration {
            name: name.into(),
            protocol: ProviderProtocol::OpenAI,
            base_url: base_url.into(),
            api_key_env: None,
            models,
        }
    }

    fn make_model(id: &str) -> DynamicModelDescriptor {
        DynamicModelDescriptor {
            id: id.into(),
            name: format!("model-{}", id),
            context_window: 4096,
            max_output_tokens: 1024,
        }
    }

    #[test]
    fn test_provider_protocol_api_name() {
        assert_eq!(ProviderProtocol::OpenAI.api_name(), "openai-completions");
        assert_eq!(ProviderProtocol::Anthropic.api_name(), "anthropic-messages");
        assert_eq!(ProviderProtocol::Gemini.api_name(), "google-generative-ai");
        assert_eq!(ProviderProtocol::Ollama.api_name(), "ollama-native");
    }

    #[test]
    fn test_validate_valid() {
        let reg = make_registration(
            "my-provider",
            "https://api.example.com",
            vec![make_model("gpt-4")],
        );
        assert!(reg.validate().is_ok());
    }

    #[test]
    fn test_validate_empty_name() {
        let reg = make_registration("", "https://api.example.com", vec![make_model("gpt-4")]);
        assert!(reg.validate().is_err());
    }

    #[test]
    fn test_validate_empty_base_url() {
        let reg = make_registration("my-provider", "", vec![make_model("gpt-4")]);
        assert!(reg.validate().is_err());
    }

    #[test]
    fn test_validate_empty_models() {
        let reg = make_registration("my-provider", "https://api.example.com", vec![]);
        assert!(reg.validate().is_err());
    }

    #[test]
    fn test_validate_empty_model_id() {
        let reg = make_registration(
            "my-provider",
            "https://api.example.com",
            vec![make_model("")],
        );
        assert!(reg.validate().is_err());
    }
}
