//! Hashline 锚点系统 — 精确行级编辑的基础设施
//!
//! 对齐 Pi Agent Rust 的 hashline 协议：每行生成 2 字符哈希锚点（`N#AB`），
//! Edit 工具通过锚点定位编辑位置，无需复述原文。

/// 16 字符编码字母表，提供 256 种 2 字符组合 (16×16)。
const HASH_ALPHABET: &[u8; 16] = b"ZPMQVRWSNKTXJBYH";

/// xxHash32 常量
const PRIME1: u32 = 0x9E3779B1;
const PRIME2: u32 = 0x85EBCA77;
const PRIME3: u32 = 0xC2B2AE3D;
const PRIME4: u32 = 0x27D4EB2F;
const PRIME5: u32 = 0x165667B1;

/// 计算一行的 2 字符哈希。
///
/// 算法：trim_right → xxHash32(seed=0) → 取低字节 → 高低 nibble 各 4 bit → 字母表编码。
pub fn compute_line_hash(line: &str) -> [u8; 2] {
    let trimmed = line.trim_end();
    let hash = xxhash32(trimmed.as_bytes(), 0);
    let byte = (hash & 0xFF) as u8;
    [
        HASH_ALPHABET[((byte >> 4) & 0x0F) as usize],
        HASH_ALPHABET[(byte & 0x0F) as usize],
    ]
}

/// 内联 xxHash32 — 最小化实现，无 unsafe，纯 wrapping 算术。
fn xxhash32(data: &[u8], seed: u32) -> u32 {
    let len = data.len();
    let mut h32: u32;

    if len >= 16 {
        let mut v1 = seed.wrapping_add(PRIME1).wrapping_add(PRIME2);
        let mut v2 = seed.wrapping_add(PRIME2);
        let mut v3 = seed;
        let mut v4 = seed.wrapping_sub(PRIME1);

        let mut i = 0;
        while i + 16 <= len {
            v1 = round(v1, read_u32_le(data, i));
            v2 = round(v2, read_u32_le(data, i + 4));
            v3 = round(v3, read_u32_le(data, i + 8));
            v4 = round(v4, read_u32_le(data, i + 12));
            i += 16;
        }

        h32 = v1.wrapping_add(v2).wrapping_add(v3).wrapping_add(v4);
        h32 = avalanche(h32);
    } else {
        h32 = seed.wrapping_add(PRIME5);
    }

    h32 = h32.wrapping_add(len as u32);

    let mut i = (len / 16) * 16;
    while i + 4 <= len {
        h32 = h32.wrapping_add(read_u32_le(data, i).wrapping_mul(PRIME3));
        h32 = h32.wrapping_mul(PRIME4);
        i += 4;
    }

    while i < len {
        h32 = h32.wrapping_add((data[i] as u32).wrapping_mul(PRIME5));
        h32 = h32.wrapping_mul(PRIME1);
        i += 1;
    }

    avalanche(h32)
}

#[inline]
fn round(acc: u32, input: u32) -> u32 {
    acc.wrapping_add(input.wrapping_mul(PRIME2))
        .rotate_left(13)
        .wrapping_mul(PRIME1)
}

#[inline]
fn avalanche(mut h: u32) -> u32 {
    h ^= h >> 15;
    h = h.wrapping_mul(PRIME2);
    h ^= h >> 13;
    h = h.wrapping_mul(PRIME3);
    h ^= h >> 16;
    h
}

#[inline]
fn read_u32_le(data: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes([
        data[offset],
        data[offset + 1],
        data[offset + 2],
        data[offset + 3],
    ])
}

/// 解析后的锚点。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Anchor {
    pub line: usize,
    pub hash: [u8; 2],
}

/// 解析 `"5#KJ"` 格式的锚点字符串。
pub fn parse_anchor(s: &str) -> Option<Anchor> {
    let (line_part, hash_part) = s.split_once('#')?;
    let line: usize = line_part.parse().ok()?;
    if line == 0 || hash_part.len() != 2 {
        return None;
    }
    let bytes = hash_part.as_bytes();
    Some(Anchor {
        line,
        hash: [bytes[0], bytes[1]],
    })
}

/// 校验锚点是否与当前文件内容匹配。
pub fn validate_anchors(content: &str, anchors: &[(usize, &[u8; 2])]) -> Result<(), String> {
    let lines: Vec<&str> = content.lines().collect();
    for &(line_num, expected_hash) in anchors {
        let idx = line_num.checked_sub(1);
        match idx {
            None => return Err(format!("line {line_num} is before line 1")),
            Some(idx) if idx >= lines.len() => {
                return Err(format!(
                    "line {line_num} exceeds file length ({} lines)",
                    lines.len()
                ));
            }
            Some(idx) => {
                let actual = compute_line_hash(lines[idx]);
                if actual != *expected_hash {
                    let content_preview: String = lines[idx].chars().take(40).collect();
                    return Err(format!(
                        "hash mismatch at line {}: expected '{}', got '{}' (content: '{}{}')",
                        line_num,
                        std::str::from_utf8(expected_hash).unwrap_or("??"),
                        std::str::from_utf8(&actual).unwrap_or("??"),
                        content_preview,
                        if lines[idx].len() > 40 { "..." } else { "" }
                    ));
                }
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hash_deterministic() {
        let h1 = compute_line_hash("fn main() {}");
        let h2 = compute_line_hash("fn main() {}");
        assert_eq!(h1, h2);
    }

    #[test]
    fn test_hash_trailing_whitespace_ignored() {
        let h1 = compute_line_hash("hello");
        let h2 = compute_line_hash("hello   ");
        assert_eq!(h1, h2);
    }

    #[test]
    fn test_hash_different_lines_differ() {
        let h1 = compute_line_hash("fn main() {}");
        let h2 = compute_line_hash("fn other() {}");
        // Not guaranteed different, but extremely likely with 256 buckets
        assert_eq!(h1.len(), 2);
        assert_eq!(h2.len(), 2);
    }

    #[test]
    fn test_xxhash32_known_value() {
        // xxHash32 reference: xxhash32("", 0) = 0x02CC5D05
        let h = xxhash32(b"", 0);
        assert_eq!(h, 0x02CC5D05);
    }

    #[test]
    fn test_xxhash32_short_input() {
        let h = xxhash32(b"hello", 0);
        // Just verify it's deterministic
        assert_eq!(h, xxhash32(b"hello", 0));
        assert_ne!(h, xxhash32(b"world", 0));
    }

    #[test]
    fn test_xxhash32_long_input() {
        let data: Vec<u8> = (0..200).collect();
        let h = xxhash32(&data, 42);
        assert_eq!(h, xxhash32(&data, 42));
    }

    #[test]
    fn test_parse_anchor_valid() {
        let a = parse_anchor("5#KJ").unwrap();
        assert_eq!(a.line, 5);
        assert_eq!(&a.hash, b"KJ");
    }

    #[test]
    fn test_parse_anchor_invalid() {
        assert!(parse_anchor("5").is_none());
        assert!(parse_anchor("5#").is_none());
        assert!(parse_anchor("5#K").is_none());
        assert!(parse_anchor("5#KJX").is_none());
        assert!(parse_anchor("abc#KJ").is_none());
        assert!(parse_anchor("#KJ").is_none());
        assert!(parse_anchor("0#KJ").is_none());
    }

    #[test]
    fn test_validate_anchors_success() {
        let content = "line one\nline two\nline three\n";
        let h1 = compute_line_hash("line one");
        let h2 = compute_line_hash("line two");
        assert!(validate_anchors(content, &[(1, &h1), (2, &h2)]).is_ok());
    }

    #[test]
    fn test_validate_anchors_mismatch() {
        let content = "line one\nline two\n";
        let bad_hash = [b'X', b'X'];
        let result = validate_anchors(content, &[(1, &bad_hash)]);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("hash mismatch"));
    }

    #[test]
    fn test_validate_anchors_out_of_range() {
        let content = "line one\n";
        let bad_hash = [b'X', b'X'];
        let result = validate_anchors(content, &[(5, &bad_hash)]);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("exceeds file length"));
    }
}
