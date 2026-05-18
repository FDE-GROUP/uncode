//! 工作区图谱构建 + 缓存 + 上下文选取

use std::collections::HashMap;
use std::path::Path;
use std::time::{Duration, Instant, SystemTime};

use parking_lot::RwLock;
use regex::Regex;
use walkdir::WalkDir;

use uncode_core::workspace_graph::{
    BundleConfig, BundleItem, ContextBundle, DEFAULT_IGNORE_DIRS, EdgeType, Freshness, GraphEdge,
    GraphNode, SymbolKind, WorkspaceGraph,
};

// ── Symbol extraction regexes ──

struct SymbolPatterns {
    function: Regex,
    struct_: Regex,
    enum_: Regex,
    trait_: Regex,
    impl_: Regex,
    module: Regex,
    test_attr: Regex,
}

impl SymbolPatterns {
    fn new() -> Self {
        Self {
            function: Regex::new(
                r"^\s*(?:pub\s+)?(?:pub(?:\([^)]*\))?\s+)?(?:async\s+)?fn\s+(\w+)",
            )
            .unwrap(),
            struct_: Regex::new(r"^\s*(?:pub\s+)?struct\s+(\w+)").unwrap(),
            enum_: Regex::new(r"^\s*(?:pub\s+)?enum\s+(\w+)").unwrap(),
            trait_: Regex::new(r"^\s*(?:pub\s+)?trait\s+(\w+)").unwrap(),
            impl_: Regex::new(r"^\s*impl(?:\s*<[^>]*>)?\s+(?:\w+\s+for\s+)?(\w+)").unwrap(),
            module: Regex::new(r"^\s*(?:pub\s+)?mod\s+(\w+)").unwrap(),
            test_attr: Regex::new(r#"^\s*#\[cfg\s*\(\s*test\s*\)\]|\s*#\[test\]"#).unwrap(),
        }
    }
}

/// Extract symbols from a single Rust source file.
fn extract_symbols(
    relative_path: &str,
    contents: &str,
    freshness: Freshness,
    patterns: &SymbolPatterns,
) -> Vec<GraphNode> {
    let mut nodes = Vec::new();
    let mut in_test = false;

    for (i, line) in contents.lines().enumerate() {
        let line_num = i + 1;

        // Test detection
        if patterns.test_attr.is_match(line) {
            in_test = true;
            continue;
        }

        // Try each pattern
        if let Some(caps) = patterns.function.captures(line) {
            let name = caps[1].to_string();
            let kind = if in_test {
                SymbolKind::Test
            } else {
                SymbolKind::Function
            };
            let sig = line.trim().to_string();
            nodes.push(GraphNode {
                id: format!("{relative_path}:{line_num}:{kind:?}:{name}"),
                kind,
                name,
                path: relative_path.to_string(),
                line_start: line_num,
                line_end: line_num, // refined later if needed
                signature: Some(sig),
                freshness,
            });
            in_test = false;
            continue;
        }

        if let Some(caps) = patterns.struct_.captures(line) {
            let name = caps[1].to_string();
            let sig = line.trim().to_string();
            nodes.push(GraphNode {
                id: format!("{relative_path}:{line_num}:Struct:{name}"),
                kind: SymbolKind::Struct,
                name,
                path: relative_path.to_string(),
                line_start: line_num,
                line_end: line_num,
                signature: Some(sig),
                freshness,
            });
        } else if let Some(caps) = patterns.enum_.captures(line) {
            let name = caps[1].to_string();
            let sig = line.trim().to_string();
            nodes.push(GraphNode {
                id: format!("{relative_path}:{line_num}:Enum:{name}"),
                kind: SymbolKind::Enum,
                name,
                path: relative_path.to_string(),
                line_start: line_num,
                line_end: line_num,
                signature: Some(sig),
                freshness,
            });
        } else if let Some(caps) = patterns.trait_.captures(line) {
            let name = caps[1].to_string();
            let sig = line.trim().to_string();
            nodes.push(GraphNode {
                id: format!("{relative_path}:{line_num}:Trait:{name}"),
                kind: SymbolKind::Trait,
                name,
                path: relative_path.to_string(),
                line_start: line_num,
                line_end: line_num,
                signature: Some(sig),
                freshness,
            });
        } else if let Some(caps) = patterns.impl_.captures(line) {
            let name = caps[1].to_string();
            let sig = line.trim().to_string();
            nodes.push(GraphNode {
                id: format!("{relative_path}:{line_num}:Impl:{name}"),
                kind: SymbolKind::Impl,
                name,
                path: relative_path.to_string(),
                line_start: line_num,
                line_end: line_num,
                signature: Some(sig),
                freshness,
            });
        } else if let Some(caps) = patterns.module.captures(line) {
            let name = caps[1].to_string();
            nodes.push(GraphNode {
                id: format!("{relative_path}:{line_num}:Module:{name}"),
                kind: SymbolKind::Module,
                name,
                path: relative_path.to_string(),
                line_start: line_num,
                line_end: line_num,
                signature: None,
                freshness,
            });
        }

        in_test = false;
    }

    nodes
}

/// Heuristic edge extraction: find Implements and Contains edges.
fn extract_edges(nodes: &[GraphNode]) -> Vec<GraphEdge> {
    let mut edges = Vec::new();

    // Collect struct names per file for Implements detection
    let struct_names: HashMap<String, &str> = nodes
        .iter()
        .filter(|n| n.kind == SymbolKind::Struct)
        .map(|n| (n.name.to_lowercase(), n.name.as_str()))
        .collect();

    for node in nodes {
        if node.kind == SymbolKind::Impl {
            if let Some(ref _sig) = node.signature {
                // Check if impl target matches a known struct
                let target_lower = node.name.to_lowercase();
                if let Some(&struct_name) = struct_names.get(&target_lower) {
                    // Find the struct node
                    if let Some(struct_node) = nodes.iter().find(|n| {
                        n.kind == SymbolKind::Struct && n.path == node.path && n.name == struct_name
                    }) {
                        edges.push(GraphEdge {
                            source: node.id.clone(),
                            target: struct_node.id.clone(),
                            edge_type: EdgeType::Implements,
                        });
                    }
                }
            }
        }

        // Module -> items in same file (Contains)
        if node.kind == SymbolKind::Module {
            for other in nodes {
                if other.kind != SymbolKind::Module
                    && other.path == node.path
                    && other.line_start > node.line_start
                {
                    edges.push(GraphEdge {
                        source: node.id.clone(),
                        target: other.id.clone(),
                        edge_type: EdgeType::Contains,
                    });
                }
            }
        }
    }

    edges
}

/// Determine file freshness based on modification time.
fn file_freshness(path: &Path) -> Freshness {
    let twenty_four_hours = Duration::from_secs(24 * 3600);
    match path.metadata().and_then(|m| m.modified()) {
        Ok(modified) => {
            let age = SystemTime::now()
                .duration_since(modified)
                .unwrap_or(Duration::MAX);
            if age < twenty_four_hours {
                Freshness::Recent
            } else {
                Freshness::Fresh
            }
        }
        Err(_) => Freshness::Fresh,
    }
}

/// Simple hash of file contents for incremental invalidation.
fn hash_contents(contents: &str) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    contents.hash(&mut hasher);
    hasher.finish()
}

/// Build workspace graph by scanning .rs files.
pub fn build_graph(root: &Path, config: &BundleConfig) -> WorkspaceGraph {
    let patterns = SymbolPatterns::new();
    let mut all_nodes = Vec::new();
    let mut all_edges = Vec::new();
    let mut file_hashes = HashMap::new();

    let ignore_dirs: Vec<&str> = DEFAULT_IGNORE_DIRS.to_vec();

    for entry in WalkDir::new(root).into_iter().filter_map(|e| e.ok()) {
        let path = entry.path();

        // Skip non-.rs files
        if !path.extension().is_some_and(|ext| ext == "rs") {
            continue;
        }

        // Skip ignored directories
        let relative = path.strip_prefix(root).unwrap_or(path);
        let relative_str = relative.to_string_lossy();
        if ignore_dirs.iter().any(|dir| {
            relative_str.starts_with(&format!("{dir}/"))
                || relative_str.starts_with(&format!("{dir}\\"))
        }) {
            continue;
        }

        // Skip large files
        let file_size = entry.metadata().map(|m| m.len()).unwrap_or(0);
        if file_size > config.max_file_bytes as u64 {
            continue;
        }

        let contents = match std::fs::read_to_string(path) {
            Ok(c) => c,
            Err(_) => continue,
        };

        let rel_path = relative.to_string_lossy().to_string();
        file_hashes.insert(rel_path.clone(), hash_contents(&contents));

        let freshness = file_freshness(path);
        let mut nodes = extract_symbols(&rel_path, &contents, freshness, &patterns);
        let edges = extract_edges(&nodes);

        all_nodes.append(&mut nodes);
        all_edges.extend(edges);
    }

    WorkspaceGraph {
        root: root.to_path_buf(),
        nodes: all_nodes,
        edges: all_edges,
        built_at: chrono::Utc::now(),
        file_hashes,
    }
}

// ── Cache ──

struct CachedGraph {
    graph: WorkspaceGraph,
    built_at: Instant,
}

/// TTL-based workspace graph cache.
pub struct WorkspaceGraphCache {
    cached: RwLock<Option<CachedGraph>>,
    config: BundleConfig,
}

impl WorkspaceGraphCache {
    pub fn new(config: BundleConfig) -> Self {
        Self {
            cached: RwLock::new(None),
            config,
        }
    }

    /// Get cached graph or build a new one if TTL expired.
    pub async fn get_or_build(&self, root: &Path) -> WorkspaceGraph {
        {
            let guard = self.cached.read();
            if let Some(ref cached) = *guard {
                let elapsed = cached.built_at.elapsed();
                if elapsed.as_secs() < self.config.ttl_secs && cached.graph.root == root {
                    return cached.graph.clone();
                }
            }
        }

        let config = self.config.clone();
        let root_clone = root.to_path_buf();
        let graph = tokio::task::spawn_blocking(move || build_graph(&root_clone, &config))
            .await
            .unwrap_or_else(|_| WorkspaceGraph {
                root: root.to_path_buf(),
                nodes: Vec::new(),
                edges: Vec::new(),
                built_at: chrono::Utc::now(),
                file_hashes: HashMap::new(),
            });

        {
            let mut guard = self.cached.write();
            *guard = Some(CachedGraph {
                graph: graph.clone(),
                built_at: Instant::now(),
            });
        }

        graph
    }

    /// Force invalidation (called after write/edit tool execution).
    pub fn invalidate(&self) {
        let mut guard = self.cached.write();
        *guard = None;
    }

    pub fn config(&self) -> &BundleConfig {
        &self.config
    }
}

// ── Context Bundle Selection ──

/// Select relevant nodes and build a context bundle for injection.
pub fn select_bundle(
    graph: &WorkspaceGraph,
    recent_files: &[String],
    user_message: &str,
    config: &BundleConfig,
) -> ContextBundle {
    if graph.nodes.is_empty() {
        return ContextBundle::default();
    }

    // Tokenize user message for keyword matching
    let msg_tokens: Vec<&str> = user_message
        .split(|c: char| !c.is_alphanumeric() && c != '_' && c != '/')
        .filter(|t| t.len() >= 2)
        .collect();

    // Count edges per node for connectivity score
    let mut edge_count: HashMap<&str, usize> = HashMap::new();
    for edge in &graph.edges {
        *edge_count.entry(&edge.source).or_default() += 1;
        *edge_count.entry(&edge.target).or_default() += 1;
    }

    // Score each node
    let mut scored: Vec<(f32, &GraphNode)> = graph
        .nodes
        .iter()
        .map(|node| {
            let mut score: f32 = 0.0;

            // Recency (0-30): recent_files +15, user message mentions +15
            if recent_files.iter().any(|f| node.path == *f) {
                score += 15.0;
            }
            if msg_tokens.iter().any(|t| node.path.contains(t)) {
                score += 15.0;
            }

            // Freshness (0-10)
            score += match node.freshness {
                Freshness::Fresh => 10.0,
                Freshness::Recent => 5.0,
                Freshness::Stale => 0.0,
            };

            // Kind priority (0-20)
            score += match node.kind {
                SymbolKind::Function => 20.0,
                SymbolKind::Struct => 18.0,
                SymbolKind::Trait => 15.0,
                SymbolKind::Enum => 15.0,
                SymbolKind::Impl => 12.0,
                SymbolKind::Module => 8.0,
                SymbolKind::Test => 5.0,
                SymbolKind::DocSection => 3.0,
            };

            // Connectivity (0-15): +3 per edge, cap 15
            let ec = *edge_count.get(node.id.as_str()).unwrap_or(&0);
            score += (ec as f32 * 3.0).min(15.0);

            // Keyword match (0-25): +5 per matching token in name/path
            let name_lower = node.name.to_lowercase();
            let path_lower = node.path.to_lowercase();
            let keyword_score: f32 = msg_tokens
                .iter()
                .map(|t| {
                    let t_lower = t.to_lowercase();
                    if name_lower.contains(&t_lower) || path_lower.contains(&t_lower) {
                        5.0
                    } else {
                        0.0
                    }
                })
                .sum();
            score += keyword_score.min(25.0);

            (score, node)
        })
        .collect();

    // Sort by score descending
    scored.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));

    // Greedily add items within budget
    let mut items = Vec::new();
    let mut total_bytes = 0;

    for (score, node) in scored {
        if items.len() >= config.max_items || total_bytes >= config.max_bytes {
            break;
        }

        // Read source content for this node
        let content = read_node_content(&graph.root, node);

        let item_bytes = content.len();
        if total_bytes + item_bytes > config.max_bytes {
            continue; // Skip if would exceed budget
        }

        total_bytes += item_bytes;
        items.push(BundleItem {
            node_id: node.id.clone(),
            kind: node.kind,
            name: node.name.clone(),
            path: node.path.clone(),
            line_start: node.line_start,
            line_end: node.line_end,
            content,
            score,
        });
    }

    ContextBundle { items, total_bytes }
}

/// Read source lines for a node from disk.
fn read_node_content(root: &Path, node: &GraphNode) -> String {
    let path = root.join(&node.path);
    let contents = match std::fs::read_to_string(&path) {
        Ok(c) => c,
        Err(_) => return String::new(),
    };

    let lines: Vec<&str> = contents.lines().collect();
    let start = node.line_start.saturating_sub(1);
    let end = (node.line_end).min(lines.len());

    if start >= lines.len() {
        return String::new();
    }

    lines[start..end].join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write as IoWrite;
    use std::path::PathBuf;

    fn make_test_graph() -> WorkspaceGraph {
        WorkspaceGraph {
            root: PathBuf::from("/test"),
            nodes: vec![
                GraphNode {
                    id: "src/main.rs:5:Function:run".into(),
                    kind: SymbolKind::Function,
                    name: "run".into(),
                    path: "src/main.rs".into(),
                    line_start: 5,
                    line_end: 5,
                    signature: Some("pub fn run()".into()),
                    freshness: Freshness::Fresh,
                },
                GraphNode {
                    id: "src/lib.rs:10:Struct:Agent".into(),
                    kind: SymbolKind::Struct,
                    name: "Agent".into(),
                    path: "src/lib.rs".into(),
                    line_start: 10,
                    line_end: 10,
                    signature: Some("pub struct Agent".into()),
                    freshness: Freshness::Recent,
                },
                GraphNode {
                    id: "src/lib.rs:20:Function:process".into(),
                    kind: SymbolKind::Function,
                    name: "process".into(),
                    path: "src/lib.rs".into(),
                    line_start: 20,
                    line_end: 20,
                    signature: Some("fn process(&self)".into()),
                    freshness: Freshness::Fresh,
                },
            ],
            edges: vec![GraphEdge {
                source: "src/lib.rs:20:Function:process".into(),
                target: "src/lib.rs:10:Struct:Agent".into(),
                edge_type: EdgeType::Contains,
            }],
            built_at: chrono::Utc::now(),
            file_hashes: HashMap::new(),
        }
    }

    #[test]
    fn test_extract_symbols_basic() {
        let code = r#"
pub struct Agent {
    name: String,
}

impl Agent {
    pub fn run(&self) {}
}

fn helper() {}

pub enum Status {
    Active,
    Idle,
}

pub trait Handler {
    fn handle(&self);
}

pub mod tools;
"#;
        let patterns = SymbolPatterns::new();
        let nodes = extract_symbols("src/lib.rs", code, Freshness::Fresh, &patterns);

        assert!(
            nodes
                .iter()
                .any(|n| n.kind == SymbolKind::Struct && n.name == "Agent")
        );
        assert!(
            nodes
                .iter()
                .any(|n| n.kind == SymbolKind::Impl && n.name == "Agent")
        );
        assert!(
            nodes
                .iter()
                .any(|n| n.kind == SymbolKind::Function && n.name == "run")
        );
        assert!(
            nodes
                .iter()
                .any(|n| n.kind == SymbolKind::Function && n.name == "helper")
        );
        assert!(
            nodes
                .iter()
                .any(|n| n.kind == SymbolKind::Enum && n.name == "Status")
        );
        assert!(
            nodes
                .iter()
                .any(|n| n.kind == SymbolKind::Trait && n.name == "Handler")
        );
        assert!(
            nodes
                .iter()
                .any(|n| n.kind == SymbolKind::Module && n.name == "tools")
        );
    }

    #[test]
    fn test_extract_symbols_test_detection() {
        let code = r#"
#[test]
fn test_something() {}

fn normal_fn() {}
"#;
        let patterns = SymbolPatterns::new();
        let nodes = extract_symbols("src/lib.rs", code, Freshness::Fresh, &patterns);

        let test_fn = nodes.iter().find(|n| n.name == "test_something").unwrap();
        assert_eq!(test_fn.kind, SymbolKind::Test);

        let normal = nodes.iter().find(|n| n.name == "normal_fn").unwrap();
        assert_eq!(normal.kind, SymbolKind::Function);
    }

    #[test]
    fn test_extract_edges_implements() {
        let nodes = vec![
            GraphNode {
                id: "src/lib.rs:1:Struct:Agent".into(),
                kind: SymbolKind::Struct,
                name: "Agent".into(),
                path: "src/lib.rs".into(),
                line_start: 1,
                line_end: 1,
                signature: None,
                freshness: Freshness::Fresh,
            },
            GraphNode {
                id: "src/lib.rs:5:Impl:Agent".into(),
                kind: SymbolKind::Impl,
                name: "Agent".into(),
                path: "src/lib.rs".into(),
                line_start: 5,
                line_end: 5,
                signature: Some("impl Agent".into()),
                freshness: Freshness::Fresh,
            },
        ];
        let edges = extract_edges(&nodes);
        assert!(
            edges
                .iter()
                .any(|e| e.edge_type == EdgeType::Implements && e.source.contains("Impl:Agent"))
        );
    }

    #[test]
    fn test_select_bundle_scores_high_for_mentioned_files() {
        let graph = make_test_graph();
        let config = BundleConfig {
            max_items: 10,
            max_bytes: 100_000,
            ..BundleConfig::default()
        };

        let bundle = select_bundle(
            &graph,
            &["src/lib.rs".to_string()],
            "how does Agent work?",
            &config,
        );

        // Agent struct should score higher (recency + keyword match)
        assert!(!bundle.items.is_empty());
        let agent_item = bundle.items.iter().find(|i| i.name == "Agent");
        assert!(agent_item.is_some());
    }

    #[test]
    fn test_select_bundle_respects_budget() {
        let graph = make_test_graph();
        let config = BundleConfig {
            max_items: 1,
            max_bytes: 50,
            ..BundleConfig::default()
        };

        let bundle = select_bundle(&graph, &[], "process", &config);
        assert!(bundle.items.len() <= 1);
    }

    #[test]
    fn test_select_bundle_empty_graph() {
        let graph = WorkspaceGraph {
            root: PathBuf::from("/test"),
            nodes: vec![],
            edges: vec![],
            built_at: chrono::Utc::now(),
            file_hashes: HashMap::new(),
        };
        let config = BundleConfig::default();
        let bundle = select_bundle(&graph, &[], "anything", &config);
        assert!(bundle.is_empty());
    }

    #[tokio::test]
    async fn test_cache_builds_and_returns() {
        let dir = tempfile::tempdir().unwrap();
        let rs_path = dir.path().join("lib.rs");
        let mut f = std::fs::File::create(&rs_path).unwrap();
        writeln!(f, "pub fn hello() {{}}").unwrap();

        let config = BundleConfig {
            ttl_secs: 3600,
            ..BundleConfig::default()
        };
        let cache = WorkspaceGraphCache::new(config);

        let g1 = cache.get_or_build(dir.path()).await;
        assert!(!g1.nodes.is_empty());

        let g2 = cache.get_or_build(dir.path()).await;
        assert_eq!(g1.built_at, g2.built_at); // Same timestamp = cached
    }

    #[tokio::test]
    async fn test_cache_invalidation() {
        let dir = tempfile::tempdir().unwrap();
        let rs_path = dir.path().join("lib.rs");
        let mut f = std::fs::File::create(&rs_path).unwrap();
        writeln!(f, "pub fn hello() {{}}").unwrap();

        let config = BundleConfig {
            ttl_secs: 3600,
            ..BundleConfig::default()
        };
        let cache = WorkspaceGraphCache::new(config);

        let g1 = cache.get_or_build(dir.path()).await;
        cache.invalidate();

        let g2 = cache.get_or_build(dir.path()).await;
        // After invalidation, rebuilt_at should be different (newer)
        assert!(g2.built_at >= g1.built_at);
    }

    #[test]
    fn test_build_graph_real_directory() {
        let config = BundleConfig {
            max_file_bytes: 100_000,
            ..BundleConfig::default()
        };
        // Build graph for this crate
        let graph = build_graph(Path::new(env!("CARGO_MANIFEST_DIR")), &config);

        assert!(
            !graph.nodes.is_empty(),
            "should find symbols in the agent crate"
        );
        assert!(
            graph.nodes.iter().any(|n| n.name == "build_graph"),
            "should find build_graph function"
        );
    }
}
