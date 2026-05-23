use std::path::{Path, PathBuf};

/// 从用户输入中提取 `@<path>` 引用（不包含 URL）
pub fn extract_file_refs(text: &str) -> Vec<String> {
    let all = extract_all_refs(text);
    all.into_iter().filter(|r| !is_url(r)).collect()
}

/// 从用户输入中提取 `@<url>` 引用
pub fn extract_url_refs(text: &str) -> Vec<String> {
    let all = extract_all_refs(text);
    all.into_iter().filter(|r| is_url(r)).collect()
}

fn extract_all_refs(text: &str) -> Vec<String> {
    let mut refs = Vec::new();
    let mut chars = text.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '@' {
            let mut path = String::new();
            while let Some(&next) = chars.peek() {
                if next.is_whitespace() || next == ')' || next == ']' || next == ',' {
                    break;
                }
                path.push(chars.next().unwrap_or(next));
            }
            if !path.is_empty() {
                refs.push(path);
            }
        }
    }
    refs
}

fn is_url(s: &str) -> bool {
    s.starts_with("http://") || s.starts_with("https://")
}

/// 将用户输入中的 `@<path>` 引用展开为文件内容
///
/// - 文件：读取内容，包裹为代码块
/// - 目录：生成目录树摘要
/// - 不存在的路径：替换为错误提示
/// - 路径限制在 working_dir 内
pub fn expand_file_refs(text: &str, working_dir: &Path) -> String {
    let refs = extract_file_refs(text);
    if refs.is_empty() {
        return text.to_string();
    }

    let mut result = text.to_string();
    for path_str in refs {
        let full_path = if Path::new(&path_str).is_absolute() {
            PathBuf::from(&path_str)
        } else {
            working_dir.join(&path_str)
        };

        // 先检查存在性
        if !full_path.exists() {
            let pattern = format!("@{path_str}");
            result = result.replace(&pattern, &format!("[错误: @{} 文件或目录不存在]", path_str));
            continue;
        }

        // 安全检查：路径必须在 working_dir 内
        let canonical_working = match working_dir.canonicalize() {
            Ok(p) => p,
            Err(_) => {
                let pattern = format!("@{path_str}");
                result =
                    result.replace(&pattern, &format!("[错误: @{} 无法解析工作目录]", path_str));
                continue;
            }
        };
        let is_safe = match full_path.canonicalize() {
            Ok(p) => p.starts_with(&canonical_working),
            Err(_) => false,
        };

        let replacement = if !is_safe {
            format!("[错误: @{} 路径在工作目录外]", path_str)
        } else if full_path.is_dir() {
            expand_directory(&full_path, &path_str)
        } else {
            match std::fs::read_to_string(&full_path) {
                Ok(content) => {
                    let ext = full_path.extension().and_then(|e| e.to_str()).unwrap_or("");
                    let truncated = truncate_content(&content, 10_000);
                    format!("<file path=\"{path_str}\">\n```{ext}\n{truncated}\n```\n</file>")
                }
                Err(e) => format!("[错误: @{path_str} 读取失败: {e}]"),
            }
        };

        // 替换 @path 为展开内容
        let pattern = format!("@{path_str}");
        result = result.replace(&pattern, &replacement);
    }
    result
}

/// 生成目录树摘要
fn expand_directory(dir: &Path, display_path: &str) -> String {
    let mut entries: Vec<String> = Vec::new();
    let max_files = 50;

    if let Ok(read_dir) = std::fs::read_dir(dir) {
        for entry in read_dir.flatten() {
            if entries.len() >= max_files {
                entries.push("... (truncated)".into());
                break;
            }
            let name = entry.file_name().to_string_lossy().to_string();
            // 跳过隐藏文件和常见忽略目录
            if name.starts_with('.') || name == "target" || name == "node_modules" {
                continue;
            }
            let prefix = if entry.path().is_dir() { "/" } else { "" };
            entries.push(format!("{name}{prefix}"));
        }
    }

    let listing = entries.join("\n  ");
    format!("<directory path=\"{display_path}\">\n  {listing}\n</directory>")
}

/// 截断内容到指定字节数
fn truncate_content(content: &str, max_bytes: usize) -> String {
    if content.len() <= max_bytes {
        return content.to_string();
    }
    let truncated = &content[..max_bytes];
    // 确保不切断 UTF-8 字符
    let truncated = truncated
        .char_indices()
        .last()
        .map(|(i, _)| &content[..i])
        .unwrap_or(truncated);
    format!(
        "{truncated}\n... [truncated, {} bytes total]",
        content.len()
    )
}

/// 将用户输入中的 `@<url>` 引用展开为抓取内容（异步）
///
/// - 仅允许 http/https 协议
/// - 禁止内网地址
/// - 限制大小 10KB，超时 10 秒
pub async fn expand_url_refs(text: &str) -> String {
    let urls = extract_url_refs(text);
    if urls.is_empty() {
        return text.to_string();
    }

    let mut result = text.to_string();
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .unwrap_or_default();

    for url in urls {
        let replacement = match fetch_url(&client, &url).await {
            Ok(content) => {
                let truncated = truncate_content(&content, 10_000);
                format!("<url href=\"{url}\">\n{truncated}\n</url>")
            }
            Err(e) => format!("[错误: @{url} 抓取失败: {e}]"),
        };
        let pattern = format!("@{url}");
        result = result.replace(&pattern, &replacement);
    }
    result
}

async fn fetch_url(client: &reqwest::Client, url: &str) -> Result<String, String> {
    // 禁止内网地址
    if let Some(host) = url_host(url)
        && is_private_host(&host)
    {
        return Err("不允许访问内网地址".into());
    }

    let response = client.get(url).send().await.map_err(|e| e.to_string())?;

    if !response.status().is_success() {
        return Err(format!("HTTP {}", response.status()));
    }

    let content_type = response
        .headers()
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_string();

    let body = response.text().await.map_err(|e| e.to_string())?;

    // 对 HTML 做简单清理：移除 script/style 标签内容
    if content_type.contains("text/html") {
        Ok(strip_html_tags(&body))
    } else {
        Ok(body)
    }
}

fn url_host(url: &str) -> Option<String> {
    // 简单解析：取 http(s):// 后第一个 / 或 : 之前的部分
    let stripped = url
        .strip_prefix("http://")
        .or_else(|| url.strip_prefix("https://"))?;
    let host_port = stripped.split('/').next().unwrap_or("");
    let host = host_port.split(':').next().unwrap_or("");
    Some(host.to_string())
}

fn is_private_host(host: &str) -> bool {
    match host {
        "localhost" | "127.0.0.1" | "::1" => true,
        _ => {
            // 10.x, 172.16-31.x, 192.168.x
            let parts: Vec<&str> = host.split('.').collect();
            if parts.len() >= 2 {
                if parts[0] == "10" {
                    return true;
                }
                if parts[0] == "172"
                    && let Ok(second) = parts[1].parse::<u8>()
                {
                    return (16..=31).contains(&second);
                }
                if parts[0] == "192" && parts[1] == "168" {
                    return true;
                }
            }
            false
        }
    }
}

/// 简单的 HTML 清理：移除 script/style 标签及其内容
fn strip_html_tags(html: &str) -> String {
    let mut result = String::with_capacity(html.len());
    let mut skip_tag = None::<String>;
    let mut in_tag = false;
    let mut tag_name = String::new();

    for ch in html.chars() {
        match (ch, skip_tag.as_deref()) {
            ('<', None) => {
                in_tag = true;
                tag_name.clear();
            }
            ('>', Some(skip)) => {
                if tag_name_eq(&tag_name, skip) {
                    // skip 结束标签
                }
                in_tag = false;
                if skip_tag.is_some() && tag_name_eq(&tag_name, &format!("/{skip}")) {
                    skip_tag = None;
                }
                tag_name.clear();
            }
            (_, Some(_)) => {
                if in_tag {
                    tag_name.push(ch);
                }
                // 跳过内容
            }
            ('>', None) => {
                in_tag = false;
                let lower = tag_name.to_lowercase();
                if lower == "script" || lower == "style" || lower == "nav" || lower == "footer" {
                    skip_tag = Some(lower);
                }
                tag_name.clear();
            }
            (_, None) if in_tag => {
                tag_name.push(ch);
            }
            (_, None) => {
                result.push(ch);
            }
        }
    }

    // 解码常见 HTML 实体
    result = result.replace("&amp;", "&");
    result = result.replace("&lt;", "<");
    result = result.replace("&gt;", ">");
    result = result.replace("&quot;", "\"");
    result = result.replace("&#39;", "'");

    // 压缩连续空白
    let mut compressed = String::with_capacity(result.len());
    let mut last_was_space = false;
    for ch in result.chars() {
        if ch.is_whitespace() {
            if !last_was_space {
                compressed.push(' ');
                last_was_space = true;
            }
        } else {
            compressed.push(ch);
            last_was_space = false;
        }
    }
    compressed
}

fn tag_name_eq(tag: &str, expected: &str) -> bool {
    tag.trim().eq_ignore_ascii_case(expected)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn temp_dir() -> PathBuf {
        let dir = std::env::temp_dir().join(format!("uncode-ctx-test-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn test_extract_file_refs_basic() {
        let refs = extract_file_refs("请看 @src/main.rs 的内容");
        assert_eq!(refs, vec!["src/main.rs"]);
    }

    #[test]
    fn test_extract_file_refs_multiple() {
        let refs = extract_file_refs("对比 @a.rs 和 @b.rs");
        assert_eq!(refs, vec!["a.rs", "b.rs"]);
    }

    #[test]
    fn test_extract_file_refs_none() {
        let refs = extract_file_refs("没有任何引用");
        assert!(refs.is_empty());
    }

    #[test]
    fn test_extract_file_refs_with_parens() {
        let refs = extract_file_refs("看 @lib.rs) 其他");
        assert_eq!(refs, vec!["lib.rs"]);
    }

    #[test]
    fn test_expand_file_no_refs() {
        let result = expand_file_refs("普通文本", Path::new("/tmp"));
        assert_eq!(result, "普通文本");
    }

    #[test]
    fn test_expand_file_missing() {
        let dir = temp_dir();
        let result = expand_file_refs("看 @nonexistent.txt", &dir);
        assert!(result.contains("文件或目录不存在"));
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn test_expand_file_reads_content() {
        let dir = temp_dir();
        fs::write(dir.join("hello.rs"), "fn main() {}").unwrap();

        let result = expand_file_refs("看 @hello.rs", &dir);
        assert!(result.contains("fn main()"));
        assert!(result.contains("```rs"));
        assert!(!result.contains("@hello.rs"));

        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn test_expand_directory() {
        let dir = temp_dir();
        fs::create_dir_all(dir.join("subdir")).unwrap();
        fs::write(dir.join("a.txt"), "aaa").unwrap();
        fs::write(dir.join("b.rs"), "bbb").unwrap();

        let result = expand_file_refs("看 @.", &dir);
        assert!(result.contains("a.txt"));
        assert!(result.contains("b.rs"));
        assert!(result.contains("subdir/"));

        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn test_truncate_content_short() {
        let result = truncate_content("hello", 100);
        assert_eq!(result, "hello");
    }

    #[test]
    fn test_truncate_content_long() {
        let long = "a".repeat(20_000);
        let result = truncate_content(&long, 100);
        assert!(result.contains("[truncated"));
        assert!(result.len() < long.len());
    }
}
