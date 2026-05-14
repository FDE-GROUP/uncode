use std::sync::Arc;

use anyhow::Context;
use clap::Parser;
use tracing_subscriber::EnvFilter;

use uncode_agent::{AgentLoop, ContextLoader, GitHubClient, SystemPromptBuilder};
use uncode_core::config::AppConfig;
use uncode_core::message::{ContentBlock, Message, Role};
use uncode_llm::registry::ProviderRegistry;
use uncode_llm::{DeepSeekDriver, GlmDriver, OllamaDriver};
use uncode_session::store::SessionStore;
use uncode_tools::registry::ToolRegistry;
use uncode_tools::{BashTool, EditTool, GrepTool, ReadTool, WriteTool};

#[derive(Parser)]
#[command(name = "uncode", about = "AI Agent Coding System")]
struct Cli {
    #[arg(short, long, default_value = "deepseek-v3")]
    model: String,

    #[arg(long)]
    session: Option<String>,

    #[arg(long)]
    issue: Option<u64>,

    #[arg(short, long)]
    interactive: bool,

    prompt: Option<String>,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .init();

    let cli = Cli::parse();
    let config = load_config()?;

    let tool_registry = Arc::new(ToolRegistry::new());
    tool_registry.register("read".to_string(), Arc::new(ReadTool::new()));
    tool_registry.register("write".to_string(), Arc::new(WriteTool));
    tool_registry.register("edit".to_string(), Arc::new(EditTool));
    tool_registry.register("grep".to_string(), Arc::new(GrepTool));
    tool_registry.register("bash".to_string(), Arc::new(BashTool::new()));

    let provider_registry = Arc::new(ProviderRegistry::new());
    register_providers(&provider_registry, &config)?;

    let session_dir = SessionStore::default_dir().context("session dir")?;
    let session_store = Arc::new(SessionStore::new(session_dir));

    let driver = provider_registry
        .get(&cli.model)
        .or_else(|| provider_registry.get("deepseek-v3"))
        .or_else(|| provider_registry.get("glm-4"))
        .or_else(|| provider_registry.get("ollama"))
        .context("no LLM driver available")?;

    let cwd = std::env::current_dir()?;
    let ctx = ContextLoader::new(cwd).load();

    let system_prompt = SystemPromptBuilder::new()
        .base("你是一个 AI 编程助手。用中文回复。")
        .add_tool_guide(&tool_registry.definitions())
        .add_context(&ctx.agents_content)
        .add_skills(&ctx.skills)
        .build();

    let mut agent = AgentLoop::new(
        driver,
        tool_registry,
        session_store,
        system_prompt,
        cli.model.clone(),
    );

    if let Some(session_id) = &cli.session {
        agent.set_session_id(session_id.clone());
    }

    if let Some(issue_number) = cli.issue {
        let token = std::env::var("GITHUB_TOKEN").context("GITHUB_TOKEN not set")?;

        let gh = GitHubClient::new(token);

        let cwd = std::env::current_dir()?;
        let dir_name = cwd
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();

        let issue = gh.fetch_issue("FDE-GROUP", &dir_name, issue_number).await?;

        let prompt = format!(
            "Please implement the following GitHub Issue:\n\nTitle: {}\n\n{}",
            issue.title, issue.body
        );

        println!(
            "Issue #{number}: {title}",
            number = issue.number,
            title = issue.title
        );
        println!("---");

        let messages = agent.run(Message::user(prompt)).await?;

        for msg in &messages {
            if msg.role == Role::Assistant || msg.role == Role::Tool {
                for block in &msg.content {
                    match block {
                        ContentBlock::Text { text } if !text.is_empty() => {
                            println!("{text}");
                        }
                        ContentBlock::ToolResult(tr) => {
                            let prefix = if tr.is_error { "error" } else { "result" };
                            println!("[{prefix}] {}", &tr.content[..tr.content.len().min(500)]);
                        }
                        _ => {}
                    }
                }
            }
        }

        println!("\n--- done ---");
        return Ok(());
    }

    if let Some(prompt) = cli.prompt {
        let messages = agent.run(Message::user(prompt)).await?;

        for msg in &messages {
            if msg.role == Role::Assistant || msg.role == Role::Tool {
                for block in &msg.content {
                    match block {
                        ContentBlock::Text { text } if !text.is_empty() => {
                            println!("{text}");
                        }
                        ContentBlock::ToolResult(tr) => {
                            println!(
                                "[{}] {}",
                                if tr.is_error { "error" } else { "result" },
                                &tr.content[..tr.content.len().min(300)]
                            );
                        }
                        _ => {}
                    }
                }
            }
        }
        return Ok(());
    }

    if cli.interactive {
        eprintln!("interactive mode not yet implemented");
        return Ok(());
    }

    Cli::parse_from(["uncode", "--help"]);
    Ok(())
}

fn load_config() -> anyhow::Result<AppConfig> {
    let config_path = dirs::config_dir()
        .unwrap_or_default()
        .join("uncode")
        .join("config.toml");

    if config_path.exists() {
        let content = std::fs::read_to_string(&config_path)?;
        let config: AppConfig = toml::from_str(&content)?;
        Ok(config)
    } else {
        Ok(AppConfig::default())
    }
}

fn register_providers(registry: &ProviderRegistry, config: &AppConfig) -> anyhow::Result<()> {
    if let Some(ref pc) = config.providers.glm {
        registry.register("glm-4".into(), Arc::new(GlmDriver::new(pc.api_key.clone())));
    }
    if let Some(ref pc) = config.providers.deepseek {
        let driver = if let Some(ref url) = pc.base_url {
            DeepSeekDriver::with_base_url(pc.api_key.clone(), url.clone())
        } else {
            DeepSeekDriver::new(pc.api_key.clone())
        };
        registry.register("deepseek-v3".into(), Arc::new(driver));
    }
    if let Some(ref oc) = config.providers.ollama {
        let driver = OllamaDriver::with_host(oc.host.clone());
        registry.register("ollama".into(), Arc::new(driver));
    }
    Ok(())
}
