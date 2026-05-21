//! URL safety checks for outbound HTTP tools (SSRF mitigation).

#[cfg(test)]
use std::cell::Cell;

#[cfg(test)]
thread_local! {
    static ALLOW_LOOPBACK_IN_TESTS: Cell<bool> = const { Cell::new(false) };
}

/// Allow loopback/localhost HTTP in the current test thread (e.g. wiremock).
#[cfg(test)]
pub(crate) fn set_allow_loopback_for_tests(allow: bool) {
    ALLOW_LOOPBACK_IN_TESTS.with(|flag| flag.set(allow));
}

#[cfg(test)]
fn loopback_allowed_in_tests() -> bool {
    ALLOW_LOOPBACK_IN_TESTS.with(|flag| flag.get())
}

/// Reject URLs whose host resolves to obvious private / loopback targets.
pub fn ensure_public_http_url(url: &str) -> Result<(), String> {
    if !url.starts_with("http://") && !url.starts_with("https://") {
        return Err("only http/https URLs are supported".into());
    }

    let host = parse_host(url).ok_or_else(|| format!("invalid URL host: {url}"))?;
    let host_lower = host.to_ascii_lowercase();

    #[cfg(test)]
    if loopback_allowed_in_tests()
        && (host_lower == "localhost"
            || host_lower.ends_with(".localhost")
            || parse_ipv4(&host_lower).is_some_and(|ip| ip[0] == 127))
    {
        return Ok(());
    }

    if host_lower == "localhost" || host_lower.ends_with(".localhost") {
        return Err(format!("blocked host (localhost): {host}"));
    }

    if host_lower.starts_with('[') {
        return check_ipv6_literal(&host_lower);
    }

    if let Some(ip) = parse_ipv4(&host_lower) {
        if is_blocked_ipv4_octets(ip) {
            return Err(format!("blocked host (private/reserved): {host}"));
        }
        return Ok(());
    }

    // Numeric-only host without dots (uncommon)
    if host_lower.chars().all(|c| c.is_ascii_digit()) && is_blocked_ipv4(&host_lower) {
        return Err(format!("blocked host (private/reserved): {host}"));
    }

    Ok(())
}

fn parse_host(url: &str) -> Option<String> {
    let rest = url
        .strip_prefix("https://")
        .or_else(|| url.strip_prefix("http://"))?;
    let authority = rest.split('/').next()?;
    let host_port = authority.rsplit('@').next()?;
    let host = host_port.split(':').next()?;
    if host.is_empty() {
        None
    } else {
        Some(host.to_string())
    }
}

fn parse_ipv4(host: &str) -> Option<[u8; 4]> {
    let mut parts = host.split('.');
    let a: u8 = parts.next()?.parse().ok()?;
    let b: u8 = parts.next()?.parse().ok()?;
    let c: u8 = parts.next()?.parse().ok()?;
    let d: u8 = parts.next()?.parse().ok()?;
    if parts.next().is_some() {
        return None;
    }
    Some([a, b, c, d])
}

fn is_blocked_ipv4(host: &str) -> bool {
    parse_ipv4(host).is_some_and(is_blocked_ipv4_octets)
}

fn is_blocked_ipv4_octets(ip: [u8; 4]) -> bool {
    matches!(
        ip,
        [0, _, _, _]
            | [10, _, _, _]
            | [127, _, _, _]
            | [169, 254, _, _]
            | [192, 168, _, _]
            | [255, 255, 255, 255]
    ) || (ip[0] == 172 && (16..=31).contains(&ip[1]))
}

fn check_ipv6_literal(host: &str) -> Result<(), String> {
    let inner = host
        .strip_prefix('[')
        .and_then(|s| s.strip_suffix(']'))
        .unwrap_or(host);
    let h = inner.to_ascii_lowercase();
    if h == "::1" || h == "0:0:0:0:0:0:0:1" {
        return Err(format!("blocked host (loopback): {host}"));
    }
    if h.starts_with("fc") || h.starts_with("fd") || h.starts_with("fe80") {
        return Err(format!("blocked host (private/link-local): {host}"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn allows_public_host() {
        ensure_public_http_url("https://example.com/doc").unwrap();
    }

    #[test]
    fn blocks_loopback() {
        assert!(ensure_public_http_url("http://127.0.0.1/").is_err());
        assert!(ensure_public_http_url("http://localhost/").is_err());
    }

    #[test]
    fn blocks_metadata_ip() {
        assert!(ensure_public_http_url("http://169.254.169.254/").is_err());
    }

    #[test]
    fn blocks_private_ranges() {
        assert!(ensure_public_http_url("http://10.0.0.1/").is_err());
        assert!(ensure_public_http_url("http://192.168.1.1/").is_err());
        assert!(ensure_public_http_url("http://172.16.0.1/").is_err());
    }
}
