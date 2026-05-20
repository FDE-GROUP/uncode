//! UTF-8 感知文本工具（基于 [`std::str`] 的字符边界，避免按字节截断）。

/// 按 Unicode 标量值个数截断；超出时在字符边界处追加 `…`（U+2026）。
///
/// 使用 [`str::char_indices`] 一次定位边界，避免 `chars().count()` 与 `take` 的双重遍历。
pub fn truncate_chars(s: &str, max_chars: usize) -> String {
    if max_chars == 0 {
        return if s.is_empty() {
            String::new()
        } else {
            "…".to_string()
        };
    }
    match s.char_indices().nth(max_chars) {
        None => s.to_string(),
        Some((byte_idx, _)) => {
            let mut out = String::with_capacity(byte_idx + '…'.len_utf8());
            out.push_str(&s[..byte_idx]);
            out.push('…');
            out
        }
    }
}

/// [`truncate_chars`]，先 [`str::trim`]；空串返回空。
pub fn truncate_chars_trimmed(s: &str, max_chars: usize) -> String {
    let t = s.trim();
    if t.is_empty() {
        String::new()
    } else {
        truncate_chars(t, max_chars)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn truncate_chars_short_unchanged() {
        assert_eq!(truncate_chars("hello", 10), "hello");
    }

    #[test]
    fn truncate_chars_respects_scalar_boundary() {
        assert_eq!(truncate_chars("你好世界", 2), "你好…");
    }

    #[test]
    fn truncate_chars_trimmed_empty() {
        assert_eq!(truncate_chars_trimmed("  \n  ", 5), "");
    }

    #[test]
    fn truncate_chars_zero_limit_is_ellipsis_only() {
        assert_eq!(truncate_chars("hi", 0), "…");
        assert_eq!(truncate_chars("", 0), "");
    }

    #[test]
    fn truncate_chars_exact_limit_no_ellipsis() {
        assert_eq!(truncate_chars("abcd", 4), "abcd");
        assert_eq!(truncate_chars("你好", 2), "你好");
    }

    #[test]
    fn truncate_chars_trimmed_preserves_inner_content() {
        assert_eq!(truncate_chars_trimmed("  hello  ", 3), "hel…");
    }
}
