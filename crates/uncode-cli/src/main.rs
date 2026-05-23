//! uncode-cli — 命令行入口
//!
//! clap 参数解析、配置加载、Agent 编排。
//! 支持单次对话、REPL、TUI、JSON-RPC 等多种交互模式。

use std::collections::HashMap;
use std::sync::Arc;

/// Output guard — 在非 TUI 模式下保护 stdout 不被 tracing 污染。
///
/// **Pi:** 对照 `output-guard.ts`：`takeOverStdout` / `restoreStdout` / `writeRawStdout`。
/// Rust 无法像 Node.js 替换 process.stdout，因此策略不同：
/// tracing 显式写 stderr，协议输出通过 `write_raw_stdout` 显式写 stdout。
mod output_guard {
    use std::io::{self, Write};

    /// 直接写 stdout（用于 JSON / RPC 协议输出）。
    pub fn write_raw_stdout(data: &[u8]) -> io::Result<()> {
        let mut stdout = io::stdout().lock();
        stdout.write_all(data)?;
        stdout.flush()?;
        Ok(())
    }
}

use anyhow::Context;
use clap::{CommandFactory, Parser, Subcommand};
use tracing_subscriber::EnvFilter;

use uncode_agent::session::store::SessionStore;
use uncode_agent::tools::{ToolLaunchConfig, ToolRegistry, register_coding_tools_and_configure};
use uncode_agent::workspace_graph::WorkspaceGraphCache;
use uncode_agent::{
    AgentLoop, ChainedToolHooks, ContextLoader, ExtensionLifecycleBridge, ExtensionToolExecutor,
    ExtensionToolHooks, GitHubClient, PermissionGate, PermissionPolicy, PermissionToolHooks,
    SystemPromptBuilder,
};
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
    #[arg(short = 'T', long = "template", value_name = "TEMPLATE")]
    template: Option<String>,

    /// 模板变量 key=value
    #[arg(long = "var")]
    var: Vec<String>,

    /// 从指定会话 fork 新分支
    #[arg(long, value_name = "SESSION_ID")]
    fork: Option<String>,

    /// 逗号分隔的工具白名单（如 read,bash,edit,write,grep）
    #[arg(long = "tools", short = 't', value_name = "TOOLS")]
    tools: Option<String>,

    /// 不向 LLM 暴露任何工具
    #[arg(long = "no-tools", conflicts_with_all = ["tools", "no_builtin_tools"])]
    no_tools: bool,

    /// 仅向 LLM 暴露扩展工具（不含 Pi 七件套；如 web_fetch / web_search）
    #[arg(long = "no-builtin-tools", conflicts_with_all = ["tools", "no_tools"])]
    no_builtin_tools: bool,

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
        .with_writer(std::io::stderr)
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

    let (api_registry, model_registry) = build_registries(&config);
    let api_registry = Arc::new(api_registry);
    let model_registry = Arc::new(model_registry);
    let api_keys = build_api_keys(&config);

    let tool_registry = Arc::new(ToolRegistry::new());
    let tools_whitelist = cli.tools.as_ref().map(|list| {
        list.split(',')
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(String::from)
            .collect::<Vec<_>>()
    });
    register_coding_tools_and_configure(
        &tool_registry,
        &api_keys,
        &ToolLaunchConfig {
            no_tools: cli.no_tools,
            no_builtin_tools: cli.no_builtin_tools,
            tools: tools_whitelist,
        },
        &config.tools,
    )
    .map_err(|e| {
        if let Some(ref list) = cli.tools {
            anyhow::anyhow!("invalid tool in --tools ({list}): {e}")
        } else {
            anyhow::anyhow!("{e}")
        }
    })?;

    let session_dir = SessionStore::default_dir().context("session dir")?;
    let session_store = Arc::new(SessionStore::new(session_dir).await?);

    let cwd = std::env::current_dir()?;
    let ctx = ContextLoader::new(cwd.clone()).load();

    let system_prompt = SystemPromptBuilder::new()
        .base(concat!(
            "你是一位专业的编程助手，运行在 UnCode 编程 Agent 框架内部。",
            "UnCode 是一个面向前线部署工程师（FDE）开发的 Agent Coding 系统。",
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

    // 默认：启动 TUI（单例 AgentLoop：同 run 内 steer，避免每次 submit 新建 loop）
    let event_rx = agent.subscribe();
    let event_tx = agent.event_sender();
    let permission_policy = Arc::new(PermissionPolicy::from_config(&config.permissions));
    let permission_gate = Arc::new(PermissionGate::new_with_policy(
        event_tx.clone(),
        tool_registry.clone(),
        permission_policy.clone(),
    ));
    let ext_registry = Arc::new(uncode_extensions::hooks::HookRegistry::new());
    let ext_hooks = Arc::new(ExtensionToolHooks::new(ext_registry.clone()));
    let tool_reg_for_cb = tool_registry.clone();

    // Pending command/shortcut registrations — consumed by TUI after spawn
    let pending_commands: Arc<
        parking_lot::Mutex<Vec<(String, String, uncode_tui::slash::CommandFn)>>,
    > = Arc::new(parking_lot::Mutex::new(Vec::new()));
    type ShortcutEntry = (
        uncode_extensions::command::ExtKeyEvent,
        Box<dyn Fn() + Send + Sync>,
    );
    let pending_shortcuts: Arc<parking_lot::Mutex<Vec<ShortcutEntry>>> =
        Arc::new(parking_lot::Mutex::new(Vec::new()));

    let cmd_cb_pending = pending_commands.clone();
    let shortcut_cb_pending = pending_shortcuts.clone();

    let tool_reg_for_unregister = tool_registry.clone();
    let cmd_cb_for_unregister = pending_commands.clone();

    let model_reg_for_provider = model_registry.clone();
    let model_reg_for_unregister = model_registry.clone();

    // Renderer registration — stored as pending, flushed after TUI engine creation.
    let pending_renderers: Arc<
        parking_lot::Mutex<Vec<uncode_extensions::renderer::ToolRenderConfig>>,
    > = Arc::new(parking_lot::Mutex::new(Vec::new()));
    let renderer_cb_pending = pending_renderers.clone();

    // Dialog channel — bridges WASM blocking thread to TUI async event loop.
    let (dialog_sender, dialog_bridge) = uncode_tui::dialog_channel::dialog_channel(16);

    let ext_api = uncode_extensions::api::ExtensionApi::with_callbacks(
        ext_registry.clone(),
        // Tool registration callback
        Some(Arc::new(
            move |name: String,
                  tool: std::sync::Arc<dyn uncode_extensions::tool::ExtensionTool>| {
                let adapter = ExtensionToolExecutor::new(tool);
                tool_reg_for_cb.register_extension_tool(name, Arc::new(adapter))
            },
        )),
        // Tool unregister callback
        Some(Arc::new(move |name: &str| -> bool {
            tool_reg_for_unregister.unregister(name)
        })),
        // Command registration callback
        Some(Arc::new(
            move |cmd: uncode_extensions::command::CommandRegistration| -> Result<(), String> {
                let desc = cmd.description.clone();
                let handler_name = cmd.name.clone();
                let handler: uncode_tui::slash::CommandFn =
                    Box::new(move |_args| format!("[extension command: {handler_name}]"));
                cmd_cb_pending.lock().push((cmd.name, desc, handler));
                Ok(())
            },
        )),
        // Command unregister callback
        Some(Arc::new(move |name: &str| -> bool {
            cmd_cb_for_unregister.lock().retain(|(n, _, _)| n != name);
            true
        })),
        // Shortcut registration callback
        Some(Arc::new(
            move |shortcut: uncode_extensions::command::ShortcutRegistration| -> Result<(), String> {
                let handler: Box<dyn Fn() + Send + Sync> =
                    Box::new(move || {});
                shortcut_cb_pending.lock().push((shortcut.key, handler));
                Ok(())
            },
        )),
        // Provider registration callback
        Some(Arc::new(
            move |reg: uncode_extensions::provider::ProviderRegistration| -> Result<(), String> {
                let api_name = reg.protocol.api_name();
                for m in &reg.models {
                    let model = uncode_ai::model::Model {
                        id: m.id.clone(),
                        name: m.name.clone(),
                        api: api_name.to_string(),
                        provider: reg.name.clone(),
                        base_url: reg.base_url.clone(),
                        context_window: m.context_window,
                        max_output_tokens: m.max_output_tokens,
                        ..uncode_ai::model::Model::default()
                    };
                    model_reg_for_provider.register(model);
                }
                tracing::info!(
                    "dynamic provider '{}' registered {} model(s)",
                    reg.name,
                    reg.models.len()
                );
                Ok(())
            },
        )),
        // Provider unregister callback
        Some(Arc::new(move |name: &str| -> bool {
            let removed = model_reg_for_unregister.unregister_by_provider(name);
            if removed > 0 {
                tracing::info!("dynamic provider '{name}' unregistered {removed} model(s)");
            }
            removed > 0
        })),
        // Renderer registration callback
        Some(Arc::new(
            move |config: uncode_extensions::renderer::ToolRenderConfig| -> Result<(), String> {
                renderer_cb_pending.lock().push(config);
                Ok(())
            },
        )),
        // Dialog callback — bridges to TUI via blocking channel
        Some(Arc::new(
            move |request: uncode_core::dialog::DialogRequest| -> Result<uncode_core::dialog::DialogResponse, String> {
                let (response_tx, response_rx) = std::sync::mpsc::channel();
                dialog_sender
                    .blocking_send(uncode_tui::dialog_channel::PendingDialog {
                        request,
                        response_tx,
                    })
                    .map_err(|e| format!("dialog channel error: {e}"))?;
                response_rx
                    .recv()
                    .map_err(|e| format!("dialog response error: {e}"))
            },
        )),
    );

    // Load WASM extensions from ~/.uncode/extensions/ (global) and .uncode/extensions/ (project)
    let ext_api = Arc::new(ext_api);
    let ext_global_dir = dirs::home_dir()
        .unwrap_or_default()
        .join(".uncode")
        .join("extensions");
    let ext_project_dir = std::env::current_dir()
        .ok()
        .map(|d| d.join(".uncode").join("extensions"));

    let ext_manager = {
        let mgr = uncode_extensions::manager::ExtensionManager::new(
            ext_registry.clone(),
            ext_api.clone(),
            ext_global_dir,
            ext_project_dir,
        );
        let report = mgr.discover_and_load_all();
        if !report.loaded.is_empty() {
            tracing::info!(
                "loaded {} extension(s), {} error(s)",
                report.loaded.len(),
                report.errors.len()
            );
        }
        for (name, err) in &report.errors {
            tracing::warn!("extension '{name}' failed to load: {err}");
        }
        Arc::new(parking_lot::Mutex::new(mgr))
    };

    let ext_bridge = ExtensionLifecycleBridge::from_arc(ext_api);
    agent.set_tool_hooks(Arc::new(ChainedToolHooks::new(vec![
        Arc::new(PermissionToolHooks::new(permission_gate.clone())),
        ext_hooks,
    ])));
    agent.set_extension_bridge(ext_bridge);
    let shared_agent = Arc::new(tokio::sync::RwLock::new(agent));

    tokio::spawn(async move {
        let mut tui = uncode_tui::TuiEngine::new();
        tui.set_permission_gate(permission_gate);
        tui.set_permission_policy(permission_policy);
        let model_ids: Vec<String> = model_registry
            .all_models()
            .into_iter()
            .map(|m| m.id.clone())
            .collect();
        tui.set_available_models(model_ids);
        tui.set_default_model(model.clone());

        // Flush pending extension command/shortcut registrations into the TUI
        for (name, desc, handler) in pending_commands.lock().drain(..) {
            tui.register_slash_command(&name, &desc, handler);
        }
        for (key, handler) in pending_shortcuts.lock().drain(..) {
            tui.register_extension_shortcut(key, handler);
        }

        // Flush pending renderer registrations
        for config in pending_renderers.lock().drain(..) {
            tui.register_custom_renderer(config.tool_name.clone(), config);
        }

        // Set dialog bridge for extension-initiated dialogs
        tui.set_dialog_bridge(dialog_bridge);

        tui.set_extension_manager(ext_manager);

        tui.run(
            event_rx,
            move |text, cancel_token, current_model, session_id, intent| {
                let agent = shared_agent.clone();
                let tx = event_tx.clone();
                tokio::spawn(async move {
                    let expanded = expand_url_refs(&text).await;
                    match intent {
                        uncode_tui::message_queue::SubmitIntent::Steer => {
                            {
                                let mut a = agent.write().await;
                                a.set_cancel_token(cancel_token);
                            }
                            let a = agent.read().await;
                            if a.is_run_active() {
                                a.steer(Message::user(expanded)).await;
                            }
                        }
                        uncode_tui::message_queue::SubmitIntent::NewRun => {
                            {
                                let mut a = agent.write().await;
                                a.set_model_id(current_model);
                                if !session_id.is_empty() {
                                    a.set_session_id(session_id);
                                }
                                a.set_cancel_token(cancel_token);
                            }
                            let a = agent.read().await;
                            if let Err(e) = a.run(Message::user(expanded)).await {
                                let session_id =
                                    a.session_id().map(|s| s.to_string()).unwrap_or_default();
                                let _ = tx.send(AgentEvent::Error {
                                    category: ErrorCategory::Llm,
                                    message: format!("{e}"),
                                    recoverable: false,
                                });
                                let _ = tx.send(AgentEvent::SessionEnd {
                                    data: Box::new(uncode_core::event::SessionEndData {
                                        session_id,
                                        total_turns: 0,
                                        total_tokens: UsageInfo::default(),
                                        exit_reason: format!("error: {e}"),
                                    }),
                                });
                            }
                        }
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

    while let Ok(event) = event_rx.recv().await {
        let json = serde_json::to_string(&event)?;
        output_guard::write_raw_stdout(json.as_bytes())?;
        output_guard::write_raw_stdout(b"\n")?;

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
    let model_registry = if config.models.is_empty() {
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
