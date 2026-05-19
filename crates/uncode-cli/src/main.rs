use std::collections::HashMap;
use std::sync::Arc;

use anyhow::Context;
use clap::{CommandFactory, Parser, Subcommand};
use tracing_subscriber::EnvFilter;

use uncode_agent::session::store::SessionStore;
use uncode_agent::tools::registry::ToolRegistry;
use uncode_agent::tools::{BashTool, EditTool, GrepTool, ReadTool, WebFetchTool, WriteTool};
use uncode_agent::workspace_graph::WorkspaceGraphCache;
use uncode_agent::{AgentLoop, ContextLoader, GitHubClient, SystemPromptBuilder};
use uncode_ai::{
    AnthropicMessagesApi, GeminiGenerativeAiApi, OllamaNativeApi, OpenAiCompletionsApi,
};
use uncode_ai::{ApiRegistry, ModelRegistry};
use uncode_core::config::AppConfig;
use uncode_core::context::{expand_file_refs, expand_url_refs};
use uncode_core::event::{AgentEvent, ErrorCategory};
use uncode_core::message::UsageInfo;
use uncode_core::message::{ContentBlock, Message, Role};
use uncode_core::template::{TemplateStore, parse_vars};

#[derive(Parser)]
#[command(name = "uncode", about = "AI Agent Coding System")]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,

    /// 模型名称
    #[arg(short, long, value_name = "MODEL")]
    model: Option<String>,

    /// 会话 ID
    #[arg(long, value_name = "SESSION_ID")]
    session: Option<String>,

    /// 继续最近会话
    #[arg(short = 'c', long = "continue")]
    continue_last: bool,

    /// GitHub Issue 编号
    #[arg(long)]
    issue: Option<u64>,

    /// REPL 模式
    #[arg(short, long)]
    repl: bool,

    /// 输出模式：interactive 或 json
    #[arg(long, default_value = "interactive", value_name = "MODE")]
    mode: String,

    /// 使用 prompt 模板
    #[arg(short = 't', long = "template", value_name = "TEMPLATE")]
    template: Option<String>,

    /// 模板变量 key=value
    #[arg(long = "var")]
    var: Vec<String>,

    /// 从指定会话 fork 新分支
    #[arg(long, value_name = "SESSION_ID")]
    fork: Option<String>,

    prompt: Option<String>,
}

#[derive(Subcommand)]
enum Commands {
    /// 列出历史会话
    Sessions {
        /// 显示全部会话（默认最近 20 条）
        #[arg(long)]
        all: bool,
        /// JSON 格式输出
        #[arg(long)]
        json: bool,
    },
    /// 列出 prompt 模板
    Templates,
    /// 列出可用模型
    Models {
        /// JSON 格式输出
        #[arg(long)]
        json: bool,
    },
    /// 导出会话为 HTML
    Export {
        /// 会话 ID
        session_id: String,
        /// 输出到文件
        #[arg(short, long)]
        output: Option<String>,
        /// 导出最近会话
        #[arg(long)]
        latest: bool,
    },
    /// 生成 shell 补全脚本
    Completions { shell: clap_complete::Shell },
    /// 启动 Platform Web 服务器
    Platform {
        /// 监听端口
        #[arg(short, long, default_value = "3000")]
        port: u16,
        /// 监听地址
        #[arg(long, default_value = "127.0.0.1")]
        host: String,
    },
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .init();

    let cli = Cli::parse();

    // 子命令处理
    if let Some(cmd) = &cli.command {
        match cmd {
            Commands::Sessions { all, json } => {
                return run_sessions(*all, *json).await;
            }
            Commands::Templates => {
                return run_templates();
            }
            Commands::Models { json } => {
                return run_models(*json);
            }
            Commands::Export {
                session_id,
                output,
                latest,
            } => {
                return run_export(session_id, output.as_deref(), *latest).await;
            }
            Commands::Completions { shell } => {
                clap_complete::generate(
                    *shell,
                    &mut Cli::command(),
                    "uncode",
                    &mut std::io::stdout(),
                );
                return Ok(());
            }
            Commands::Platform { port, host } => {
                return run_platform(host, *port);
            }
        }
    }

    let config = load_config()?;
    let model = cli.model.clone().unwrap_or_else(|| config.model.clone());

    let tool_registry = Arc::new(ToolRegistry::new());
    tool_registry.register("read".to_string(), Arc::new(ReadTool::new()));
    tool_registry.register("write".to_string(), Arc::new(WriteTool));
    tool_registry.register("edit".to_string(), Arc::new(EditTool));
    tool_registry.register("grep".to_string(), Arc::new(GrepTool));
    tool_registry.register("bash".to_string(), Arc::new(BashTool::new()));
    tool_registry.register("web_fetch".to_string(), Arc::new(WebFetchTool::new()));

    let (api_registry, model_registry) = build_registries(&config);
    let api_registry = Arc::new(api_registry);
    let model_registry = Arc::new(model_registry);
    let api_keys = build_api_keys(&config);

    // Web Search — 仅在 Tavily API Key 配置时注册
    if let Some(key) = api_keys.get("tavily")
        && let Some(tool) = uncode_agent::tools::WebSearchTool::try_new(key)
    {
        tool_registry.register("web_search".to_string(), Arc::new(tool));
    }

    let session_dir = SessionStore::default_dir().context("session dir")?;
    let session_store = Arc::new(SessionStore::new(session_dir).await?);

    let cwd = std::env::current_dir()?;
    let ctx = ContextLoader::new(cwd.clone()).load();

    let system_prompt = SystemPromptBuilder::new()
        .base(concat!(
            "你是一位专业的编程助手，运行在 UnCode 编程 Agent 框架内部。",
            "UnCode 是一个面向前线部署工程师（FDE）开发的 Agent Coding 工具。",
            "你帮助用户完成代码读写、项目分析、问题排查、命令执行等工程任务。",
            "你可以读取和编辑文件、搜索代码库、执行 Shell 命令、管理项目，",
            "拥有丰富的工具集来完成各类软件工程任务。\n\n",
            "用中文回复。遇到需要分析代码的任务时，请主动使用工具读取文件。"
        ))
        .add_working_dir(&cwd)
        .add_tool_guide(&tool_registry.definitions())
        .add_context(&ctx.agents_content)
        .add_skills(&ctx.skills)
        .build();

    let session_opt = if cli.session.is_some() {
        if cli.continue_last {
            eprintln!("warning: --session 和 --continue 同时指定，使用 --session");
        }
        cli.session.clone()
    } else if cli.continue_last {
        match session_store.find_most_recent().await.context("查找会话")? {
            Some(session) => {
                eprintln!(
                    "继续会话: {} ({})",
                    session.id,
                    session.title.as_deref().unwrap_or("无标题")
                );
                Some(session.id)
            }
            None => {
                eprintln!("没有找到历史会话，将创建新会话。");
                None
            }
        }
    } else {
        None
    };

    // Workspace graph cache
    let wg_config = &config.workspace_graph;
    let graph_cache = if wg_config.enabled {
        let bundle_config = uncode_core::workspace_graph::BundleConfig {
            enabled: wg_config.enabled,
            ttl_secs: wg_config.ttl_secs,
            max_items: wg_config.max_items,
            max_bytes: wg_config.max_bytes,
            max_file_bytes: wg_config.max_file_bytes,
        };
        Some(Arc::new(WorkspaceGraphCache::new(bundle_config)))
    } else {
        None
    };

    let mut agent = AgentLoop::new(
        api_registry.clone(),
        model_registry.clone(),
        api_keys.clone(),
        tool_registry.clone(),
        session_store.clone(),
        system_prompt.clone(),
        model.clone(),
    );

    if let Some(cache) = graph_cache.clone() {
        agent.set_graph_cache(cache);
    }

    if let Some(session_id) = &session_opt {
        agent.set_session_id(session_id.clone());
    }

    // --mode rpc: start JSON-RPC server on stdio
    if cli.mode == "rpc" {
        return run_rpc_mode(session_store, model_registry, agent).await;
    }

    // --issue：一次性执行后退出
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
        print_messages(&messages);
        println!("\n--- done ---");
        return Ok(());
    }

    if let Some(ref prompt) = cli.prompt {
        let resolved = resolve_prompt(&cli, prompt.clone(), &cwd).await;
        if cli.mode == "json" {
            return run_json_mode(agent, resolved).await;
        }
        let messages = agent.run(Message::user(resolved)).await?;
        print_messages(&messages);
        return Ok(());
    }

    // --template without prompt: execute template directly
    if let Some(template_name) = &cli.template {
        let store = TemplateStore::load();
        let vars = parse_vars(&cli.var);
        let prompt = store
            .render(template_name, &vars)
            .context(format!("模板 '{template_name}' 不存在"))?;
        println!("模板: {template_name}");
        println!("---");
        let messages = agent.run(Message::user(prompt)).await?;
        print_messages(&messages);
        println!("\n--- done ---");
        return Ok(());
    }

    // --fork：从指定会话原地分支
    if let Some(fork_id) = &cli.fork {
        let reason = cli.prompt.as_deref().unwrap_or("fork from CLI");
        let leaf_id = session_store.get_leaf_id(fork_id).await?;
        let target_entry = match &leaf_id {
            Some(id) => id.clone(),
            None => {
                anyhow::bail!("会话 {fork_id} 无叶节点，无法分支");
            }
        };
        uncode_agent::branch_summarization::branch_with_summary(
            &session_store,
            fork_id,
            &target_entry,
            reason,
        )
        .await?;
        eprintln!("原地分支: session:{fork_id} -> entry:{target_entry}");
        agent.set_session_id(fork_id.clone());

        let prompt_text = cli
            .prompt
            .clone()
            .unwrap_or_else(|| "继续从这个分叉开发".to_string());
        let expanded = expand_file_refs(&prompt_text, &cwd);
        let expanded = expand_url_refs(&expanded).await;
        let messages = agent.run(Message::user(expanded)).await?;
        print_messages(&messages);
        println!("\n--- done ---");
        return Ok(());
    }

    // --repl：纯文本 REPL 模式
    if cli.repl {
        use std::io::{BufRead, Write};
        let stdin = std::io::stdin();
        let mut reader = stdin.lock();
        let mut line = String::new();
        loop {
            print!("> ");
            std::io::stdout().flush()?;
            line.clear();
            if reader.read_line(&mut line)? == 0 {
                break;
            }
            let input = line.trim().to_string();
            if input.is_empty() {
                continue;
            }
            if input == "/quit" {
                break;
            }
            let expanded = expand_file_refs(&input, &cwd);
            let expanded = expand_url_refs(&expanded).await;
            let messages = agent.run(Message::user(expanded)).await?;
            print_messages(&messages);
        }
        return Ok(());
    }

    // 默认：启动 TUI
    let event_rx = agent.subscribe();
    let event_tx = agent.event_sender();
    let ar_tui = api_registry.clone();
    let mr_tui = model_registry.clone();
    let ak_tui = api_keys.clone();
    let tools_tui = tool_registry.clone();
    let store_tui = session_store.clone();
    let tui_system_prompt = system_prompt.clone();

    tokio::spawn(async move {
        let mut tui = uncode_tui::TuiEngine::new();
        let model_ids: Vec<String> = mr_tui
            .all_models()
            .into_iter()
            .map(|m| m.id.clone())
            .collect();
        tui.set_available_models(model_ids);
        tui.set_default_model(model.clone());
        tui.run(
            event_rx,
            move |text, cancel_token, current_model, session_id| {
                let ar = ar_tui.clone();
                let mr = mr_tui.clone();
                let ak = ak_tui.clone();
                let t = tools_tui.clone();
                let s = store_tui.clone();
                let tx = event_tx.clone();
                let sp = tui_system_prompt.clone();
                tokio::spawn(async move {
                    let expanded = expand_url_refs(&text).await;
                    let mut a = AgentLoop::with_event_sender(
                        ar,
                        mr,
                        ak,
                        t,
                        s,
                        sp,
                        current_model,
                        tx.clone(),
                    );
                    if !session_id.is_empty() {
                        a.set_session_id(session_id);
                    }
                    a.set_cancel_token(cancel_token);
                    if let Err(e) = a.run(Message::user(expanded)).await {
                        let _ = tx.send(AgentEvent::Error {
                            category: ErrorCategory::Llm,
                            message: format!("{e}"),
                            recoverable: false,
                        });
                        let _ = tx.send(AgentEvent::TurnEnd {
                            turn: 0,
                            usage: UsageInfo::default(),
                        });
                    }
                });
            },
        )
        .await;
    })
    .await?;

    Ok(())
}

async fn run_json_mode(agent: AgentLoop, prompt: String) -> anyhow::Result<()> {
    let mut event_rx = agent.subscribe();

    let agent_handle = tokio::spawn(async move { agent.run(Message::user(prompt)).await });

    use tokio::io::AsyncWriteExt;
    let mut stdout = tokio::io::stdout();
    while let Ok(event) = event_rx.recv().await {
        let json = serde_json::to_string(&event)?;
        stdout.write_all(json.as_bytes()).await?;
        stdout.write_all(b"\n").await?;
        stdout.flush().await?;

        if matches!(event, AgentEvent::SessionEnd { .. }) {
            break;
        }
    }

    let _ = agent_handle.await?;
    Ok(())
}

async fn run_rpc_mode(
    session_store: Arc<SessionStore>,
    model_registry: Arc<ModelRegistry>,
    agent: AgentLoop,
) -> anyhow::Result<()> {
    let server = Arc::new(uncode_rpc::RpcServer::new());

    // Register core commands
    uncode_rpc::register_core_commands(&server, session_store, model_registry).await;

    // Forward agent events as JSON-RPC notifications
    let event_rx = agent.subscribe();
    let server_clone = server.clone();
    tokio::spawn(async move {
        server_clone.forward_events(event_rx).await;
    });

    // Serve stdio
    server.serve().await
}

fn run_templates() -> anyhow::Result<()> {
    let store = TemplateStore::load();
    let list = store.list();
    if list.is_empty() {
        println!("没有可用模板。");
        return Ok(());
    }

    println!("{:<15} DESCRIPTION", "NAME");
    println!("{}", "-".repeat(60));
    for t in &list {
        let vars = if t.variables.is_empty() {
            String::new()
        } else {
            format!(" [{}]", t.variables.join(", "))
        };
        println!("{:<15} {}{}", t.name, t.description, vars);
    }
    Ok(())
}

fn run_models(json_output: bool) -> anyhow::Result<()> {
    let config = load_config()?;
    let (_, model_registry) = build_registries(&config);
    let api_keys = build_api_keys(&config);

    let models = model_registry.all_models();

    if json_output {
        let output: Vec<serde_json::Value> = models
            .iter()
            .map(|m| {
                let configured = api_keys.contains_key(&m.provider) || m.provider == "ollama";
                serde_json::json!({
                    "id": m.id,
                    "provider": m.provider,
                    "name": m.name,
                    "context_window": m.context_window,
                    "configured": configured,
                })
            })
            .collect();
        println!("{}", serde_json::to_string_pretty(&output)?);
        return Ok(());
    }

    println!("{:<15} {:<20} {:<12} STATUS", "PROVIDER", "MODEL", "CTX");
    println!("{}", "-".repeat(70));
    for m in models {
        let configured = api_keys.contains_key(&m.provider) || m.provider == "ollama";
        let status = if configured { "✅" } else { "❌" };
        let ctx = format!("{}k", m.context_window / 1000);
        println!("{:<15} {:<20} {:<12} {status}", m.provider, m.id, ctx);
    }
    Ok(())
}

async fn run_export(session_id: &str, output: Option<&str>, latest: bool) -> anyhow::Result<()> {
    use uncode_agent::session::export::export_html;

    let session_dir = SessionStore::default_dir().context("session dir")?;
    let store = SessionStore::new(session_dir).await?;

    let sid = if latest {
        let recent = store
            .find_most_recent()
            .await
            .context("查找会话")?
            .context("没有历史会话")?;
        eprintln!(
            "导出会话: {} ({})",
            recent.id,
            recent.title.as_deref().unwrap_or("无标题")
        );
        recent.id
    } else {
        session_id.to_string()
    };

    let header = store.read_header(&sid).await.context("读取会话头")?;
    let entries = store.load_entries(&sid).await.context("读取会话内容")?;

    let html = export_html(&header, &entries, &[]);

    if let Some(path) = output {
        std::fs::write(path, &html)?;
        eprintln!("已导出到: {path}");
    } else {
        println!("{html}");
    }
    Ok(())
}

async fn run_sessions(show_all: bool, json_output: bool) -> anyhow::Result<()> {
    let session_dir = SessionStore::default_dir().context("session dir")?;
    let store = SessionStore::new(session_dir).await?;

    let mut sessions = store.list_sessions().await?;
    sessions.sort_by_key(|b| std::cmp::Reverse(b.updated_at));

    if !show_all {
        sessions.truncate(20);
    }

    if json_output {
        println!("{}", serde_json::to_string_pretty(&sessions)?);
        return Ok(());
    }

    if sessions.is_empty() {
        println!("没有历史会话。");
        return Ok(());
    }

    // 表格头
    println!(
        "{:<12} {:<30} {:<15} {:>8}",
        "ID", "TITLE", "MODEL", "UPDATED"
    );
    println!("{}", "-".repeat(70));

    for s in &sessions {
        let id = if s.id.len() > 10 { &s.id[..10] } else { &s.id };
        let title = s
            .title
            .as_deref()
            .unwrap_or("无标题")
            .chars()
            .take(28)
            .collect::<String>();
        let model = s.model.chars().take(13).collect::<String>();
        let age = format_relative_time(s.updated_at);
        println!("{id:<12} {title:<30} {model:<15} {age:>8}");
    }

    Ok(())
}

fn format_relative_time(dt: chrono::DateTime<chrono::Utc>) -> String {
    let now: chrono::DateTime<chrono::Utc> = chrono::Utc::now();
    let diff = now.signed_duration_since(dt);
    if diff.num_minutes() < 1 {
        "刚刚".into()
    } else if diff.num_hours() < 1 {
        format!("{}分钟前", diff.num_minutes())
    } else if diff.num_days() < 1 {
        format!("{}小时前", diff.num_hours())
    } else if diff.num_weeks() < 1 {
        format!("{}天前", diff.num_days())
    } else {
        format!("{}周前", diff.num_weeks())
    }
}

fn load_config() -> anyhow::Result<AppConfig> {
    let config_path = dirs::config_dir()
        .unwrap_or_default()
        .join("uncode")
        .join("config.json");
    if config_path.exists() {
        let content = std::fs::read_to_string(&config_path)?;
        let config: AppConfig = serde_json::from_str(&content)?;
        Ok(config)
    } else {
        Ok(AppConfig::default())
    }
}

fn print_messages(messages: &[Message]) {
    for msg in messages {
        if msg.role == Role::Assistant || msg.role == Role::Tool {
            for block in &msg.content {
                match block {
                    ContentBlock::Text { text } if !text.is_empty() => println!("{text}"),
                    ContentBlock::ToolResult(tr) => {
                        let prefix = if tr.is_error { "error" } else { "result" };
                        let preview = tr.content.get(..300).unwrap_or(&tr.content);
                        println!("[{prefix}] {preview}");
                    }
                    _ => {}
                }
            }
        }
    }
}

fn build_registries(config: &AppConfig) -> (ApiRegistry, ModelRegistry) {
    let mut api_registry = ApiRegistry::new();
    api_registry.register(Arc::new(OpenAiCompletionsApi::new()));
    api_registry.register(Arc::new(AnthropicMessagesApi::new()));
    api_registry.register(Arc::new(GeminiGenerativeAiApi::new()));
    api_registry.register(Arc::new(OllamaNativeApi::new()));

    // config.models 非空时完全替代内置列表，否则用内置列表
    let mut model_registry = if config.models.is_empty() {
        ModelRegistry::from_builtin()
    } else {
        use uncode_core::model::Model;
        let models: Vec<Model> = config.models.iter().map(Model::from_model_config).collect();
        ModelRegistry::from_models(models)
    };

    // Override Ollama host if configured
    if let Some(ref oc) = config.providers.ollama {
        let base = oc.host.trim_end_matches("/v1").to_string();
        model_registry.override_base_url("ollama", &base);
    }

    // Override provider base URLs from config
    if let Some(ref pc) = config.providers.deepseek
        && let Some(ref url) = pc.base_url
    {
        model_registry.override_base_url("deepseek", url);
    }
    if let Some(ref pc) = config.providers.glm
        && let Some(ref url) = pc.base_url
    {
        model_registry.override_base_url("glm", url);
    }
    if let Some(ref pc) = config.providers.openai
        && let Some(ref url) = pc.base_url
    {
        model_registry.override_base_url("openai", url);
    }
    if let Some(ref pc) = config.providers.anthropic
        && let Some(ref url) = pc.base_url
    {
        model_registry.override_base_url("anthropic", url);
    }
    if let Some(ref pc) = config.providers.gemini
        && let Some(ref url) = pc.base_url
    {
        model_registry.override_base_url("gemini", url);
    }
    if let Some(ref pc) = config.providers.openrouter
        && let Some(ref url) = pc.base_url
    {
        model_registry.override_base_url("openrouter", url);
    }

    // Merge user_models (advanced config with api/compat overrides)
    if !config.user_models.is_empty() {
        use uncode_core::model::Model;
        let user_models: Vec<Model> = config
            .user_models
            .iter()
            .map(Model::from_user_config)
            .collect();
        model_registry.merge_user_models(user_models);
    }

    (api_registry, model_registry)
}

fn build_api_keys(config: &AppConfig) -> HashMap<String, String> {
    let mut keys = HashMap::new();
    if let Some(ref pc) = config.providers.deepseek {
        keys.insert("deepseek".into(), pc.api_key.clone());
    }
    if let Some(ref pc) = config.providers.glm {
        keys.insert("glm".into(), pc.api_key.clone());
    }
    if let Some(ref pc) = config.providers.openai {
        keys.insert("openai".into(), pc.api_key.clone());
    }
    if let Some(ref pc) = config.providers.anthropic {
        keys.insert("anthropic".into(), pc.api_key.clone());
    }
    if let Some(ref pc) = config.providers.gemini {
        keys.insert("gemini".into(), pc.api_key.clone());
    }
    if let Some(ref pc) = config.providers.openrouter {
        keys.insert("openrouter".into(), pc.api_key.clone());
    }
    if let Some(ref pc) = config.providers.tavily {
        keys.insert("tavily".into(), pc.api_key.clone());
    }

    // Extract api_keys from user-defined models
    for um in &config.user_models {
        if let Some(ref key) = um.api_key {
            keys.insert(um.provider.clone(), key.clone());
        }
    }

    keys
}

async fn resolve_prompt(cli: &Cli, prompt: String, cwd: &std::path::Path) -> String {
    if let Some(template_name) = &cli.template {
        let store = TemplateStore::load();
        let vars = parse_vars(&cli.var);
        if let Some(rendered) = store.render(template_name, &vars) {
            let expanded = expand_file_refs(&prompt, cwd);
            let expanded = expand_url_refs(&expanded).await;
            return format!("{rendered}\n\n{expanded}");
        }
    }
    let expanded = expand_file_refs(&prompt, cwd);
    expand_url_refs(&expanded).await
}

fn run_platform(host: &str, port: u16) -> anyhow::Result<()> {
    let current_exe = std::env::current_exe()?;
    let exe_dir = current_exe.parent().context("无法确定可执行文件目录")?;

    let platform_bin = exe_dir.join("uncode-platform");
    if !platform_bin.exists() {
        anyhow::bail!(
            "未找到 uncode-platform 二进制文件。请确认已编译：cargo build --release -p uncode-platform"
        );
    }

    let frontend_dir = std::env::var("UNCODE_FRONTEND_DIR").unwrap_or_else(|_| {
        let cwd = std::env::current_dir().unwrap_or_default();
        cwd.join("apps/platform/dist").to_string_lossy().to_string()
    });

    eprintln!("启动 Platform 服务器: http://{host}:{port}");
    eprintln!("前端目录: {frontend_dir}");

    let status = std::process::Command::new(platform_bin)
        .env("UNCODE_FRONTEND_DIR", &frontend_dir)
        .args(["--host", host, "--port", &port.to_string()])
        .status()
        .context("启动 uncode-platform 失败")?;

    if !status.success() {
        anyhow::bail!("uncode-platform 退出: {status}");
    }
    Ok(())
}
