//! Semantic Workspace Graph — 工作区代码符号图谱 + 上下文注入
//!
//! 扫描工作区 .rs 文件，提取函数/结构体/枚举等符号为图谱节点，
//! 按相关性评分选取 ≤N 条 ≤M KB 注入 LLM 上下文。

use std::collections::HashMap;
use std::path::PathBuf;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::message::Message;

/// 符号类型
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum SymbolKind {
    Function,
    Struct,
    Enum,
    Trait,
    Impl,
    Module,
    Test,
    DocSection,
}

/// 文件新鲜度
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Freshness {
    Fresh,
    Recent,
    Stale,
}

/// 边类型
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum EdgeType {
    Calls,
    Contains,
    Implements,
    References,
}

/// 图谱节点 — 一个代码符号
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphNode {
    /// 稳定 key: "{relative_path}:{line_start}:{kind}:{name}"
    pub id: String,
    pub kind: SymbolKind,
    pub name: String,
    /// 相对于 workspace root 的路径
    pub path: String,
    pub line_start: usize,
    pub line_end: usize,
    pub signature: Option<String>,
    pub freshness: Freshness,
}

/// 图谱边 — 节点间关系
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphEdge {
    pub source: String,
    pub target: String,
    pub edge_type: EdgeType,
}

/// 完整的工作区图谱
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkspaceGraph {
    pub root: PathBuf,
    pub nodes: Vec<GraphNode>,
    pub edges: Vec<GraphEdge>,
    pub built_at: DateTime<Utc>,
    /// path → fnv hash of content, for incremental invalidation
    pub file_hashes: HashMap<String, u64>,
}

/// 上下文注入项
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BundleItem {
    pub node_id: String,
    pub kind: SymbolKind,
    pub name: String,
    pub path: String,
    pub line_start: usize,
    pub line_end: usize,
    /// 实际源码行
    pub content: String,
    /// 相关性评分
    pub score: f32,
}

/// 上下文注入选取结果
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ContextBundle {
    pub items: Vec<BundleItem>,
    pub total_bytes: usize,
}

impl ContextBundle {
    /// 渲染为 system Message，注入 LLM 上下文
    pub fn to_system_message(&self) -> Message {
        use std::fmt::Write;
        let mut parts = String::from("## Workspace Context\n\n");

        for item in &self.items {
            let kind_label = match item.kind {
                SymbolKind::Function => "fn",
                SymbolKind::Struct => "struct",
                SymbolKind::Enum => "enum",
                SymbolKind::Trait => "trait",
                SymbolKind::Impl => "impl",
                SymbolKind::Module => "mod",
                SymbolKind::Test => "test",
                SymbolKind::DocSection => "doc",
            };
            let _ = write!(
                parts,
                "<symbol kind=\"{kind_label}\" path=\"{}\" lines=\"{}-{}\">\n",
                item.path, item.line_start, item.line_end
            );
            parts.push_str(&item.content);
            if !item.content.ends_with('\n') {
                parts.push('\n');
            }
            parts.push_str("</symbol>\n\n");
        }

        Message::system(parts)
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }
}

/// 工作区图谱配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BundleConfig {
    #[serde(default = "default_enabled")]
    pub enabled: bool,
    #[serde(default = "default_ttl_secs")]
    pub ttl_secs: u64,
    #[serde(default = "default_max_items")]
    pub max_items: usize,
    #[serde(default = "default_max_bytes")]
    pub max_bytes: usize,
    #[serde(default = "default_max_file_bytes")]
    pub max_file_bytes: usize,
}

impl Default for BundleConfig {
    fn default() -> Self {
        Self {
            enabled: default_enabled(),
            ttl_secs: default_ttl_secs(),
            max_items: default_max_items(),
            max_bytes: default_max_bytes(),
            max_file_bytes: default_max_file_bytes(),
        }
    }
}

fn default_enabled() -> bool {
    true
}
fn default_ttl_secs() -> u64 {
    21600 // 6 hours
}
fn default_max_items() -> usize {
    16
}
fn default_max_bytes() -> usize {
    16384 // 16KB
}
fn default_max_file_bytes() -> usize {
    100_000
}

/// 跳过的目录前缀
pub const DEFAULT_IGNORE_DIRS: &[&str] = &["target", "node_modules", ".git"];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bundle_config_defaults() {
        let config = BundleConfig::default();
        assert!(config.enabled);
        assert_eq!(config.ttl_secs, 21600);
        assert_eq!(config.max_items, 16);
        assert_eq!(config.max_bytes, 16384);
        assert_eq!(config.max_file_bytes, 100_000);
    }

    #[test]
    fn test_context_bundle_to_system_message() {
        let bundle = ContextBundle {
            items: vec![BundleItem {
                node_id: "src/main.rs:10:Function:run".into(),
                kind: SymbolKind::Function,
                name: "run".into(),
                path: "src/main.rs".into(),
                line_start: 10,
                line_end: 15,
                content: "fn run() {\n    println!(\"hello\");\n}".into(),
                score: 0.9,
            }],
            total_bytes: 100,
        };

        let msg = bundle.to_system_message();
        let text = match &msg.content[0] {
            crate::message::ContentBlock::Text { text } => text.clone(),
            _ => String::new(),
        };
        assert!(text.contains("## Workspace Context"));
        assert!(text.contains(r#"kind="fn""#));
        assert!(text.contains("src/main.rs"));
        assert!(text.contains("fn run()"));
    }

    #[test]
    fn test_context_bundle_empty() {
        let bundle = ContextBundle::default();
        assert!(bundle.is_empty());
        assert_eq!(bundle.total_bytes, 0);
    }

    #[test]
    fn test_symbol_kind_serde_roundtrip() {
        let kinds = vec![
            SymbolKind::Function,
            SymbolKind::Struct,
            SymbolKind::Enum,
            SymbolKind::Trait,
            SymbolKind::Impl,
            SymbolKind::Module,
            SymbolKind::Test,
            SymbolKind::DocSection,
        ];
        for kind in kinds {
            let json = serde_json::to_string(&kind).unwrap();
            let parsed: SymbolKind = serde_json::from_str(&json).unwrap();
            assert_eq!(kind, parsed);
        }
    }

    #[test]
    fn test_freshness_serde_roundtrip() {
        let variants = vec![Freshness::Fresh, Freshness::Recent, Freshness::Stale];
        for v in variants {
            let json = serde_json::to_string(&v).unwrap();
            let parsed: Freshness = serde_json::from_str(&json).unwrap();
            assert_eq!(v, parsed);
        }
    }

    #[test]
    fn test_edge_type_serde_roundtrip() {
        let variants = vec![
            EdgeType::Calls,
            EdgeType::Contains,
            EdgeType::Implements,
            EdgeType::References,
        ];
        for v in variants {
            let json = serde_json::to_string(&v).unwrap();
            let parsed: EdgeType = serde_json::from_str(&json).unwrap();
            assert_eq!(v, parsed);
        }
    }

    #[test]
    fn test_default_ignore_dirs_not_empty() {
        assert!(!DEFAULT_IGNORE_DIRS.is_empty());
        assert!(DEFAULT_IGNORE_DIRS.contains(&"node_modules"));
        assert!(DEFAULT_IGNORE_DIRS.contains(&".git"));
    }

    #[test]
    fn test_graph_node_serde() {
        let node = GraphNode {
            id: "src/main.rs:10:Function:run".into(),
            kind: SymbolKind::Function,
            name: "run".into(),
            path: "src/main.rs".into(),
            line_start: 10,
            line_end: 15,
            signature: Some("fn run()".into()),
            freshness: Freshness::Fresh,
        };
        let json = serde_json::to_string(&node).unwrap();
        let parsed: GraphNode = serde_json::from_str(&json).unwrap();
        assert_eq!(node.id, parsed.id);
        assert_eq!(node.kind, parsed.kind);
        assert_eq!(node.name, parsed.name);
        assert_eq!(node.path, parsed.path);
        assert_eq!(node.line_start, parsed.line_start);
        assert_eq!(node.line_end, parsed.line_end);
        assert_eq!(node.signature, parsed.signature);
        assert_eq!(node.freshness, parsed.freshness);
    }

    #[test]
    fn test_graph_edge_serde() {
        let edge = GraphEdge {
            source: "n1".into(),
            target: "n2".into(),
            edge_type: EdgeType::Calls,
        };
        let json = serde_json::to_string(&edge).unwrap();
        let parsed: GraphEdge = serde_json::from_str(&json).unwrap();
        assert_eq!(edge.source, parsed.source);
        assert_eq!(edge.target, parsed.target);
        assert_eq!(edge.edge_type, parsed.edge_type);
    }

    #[test]
    fn test_workspace_graph_serde() {
        let graph = WorkspaceGraph {
            root: PathBuf::from("/tmp"),
            nodes: vec![],
            edges: vec![],
            built_at: Utc::now(),
            file_hashes: HashMap::new(),
        };
        let json = serde_json::to_string(&graph).unwrap();
        let parsed: WorkspaceGraph = serde_json::from_str(&json).unwrap();
        assert_eq!(graph.root, parsed.root);
        assert!(parsed.nodes.is_empty());
        assert!(parsed.edges.is_empty());
        assert!(parsed.file_hashes.is_empty());
    }
}
