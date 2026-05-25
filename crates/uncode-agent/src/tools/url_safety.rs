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
#[must_use]
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
    // Handle IPv6 bracket notation: [::1]:8080
    if let Some(end) = host_port.find(']') {
        let bracketed = &host_port[..=end];
        return Some(bracketed.to_string());
    }
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
        // CGNAT / Shared address space (RFC 6598)
        || (ip[0] == 100 && (64..=127).contains(&ip[1]))
        // IETF Protocol Assignments, TEST-NET-1/2/3, IPv6-to-IPv4 relay
        || matches!(ip, [192, 0, 0, _] | [192, 0, 2, _] | [198, 51, 100, _] | [203, 0, 113, _] | [192, 88, 99, _])
        // Multicast, Reserved, Limited Broadcast
        || (224..=239).contains(&ip[0])
        || ip[0] >= 240
}

fn check_ipv6_literal(host: &str) -> Result<(), String> {
    let inner = host
        .strip_prefix('[')
        .and_then(|s| s.strip_suffix(']'))
        .unwrap_or(host);
    let h = inner.to_ascii_lowercase();
    // Loopback
    if h == "::1" || h == "0:0:0:0:0:0:0:1" {
        return Err(format!("blocked host (loopback): {host}"));
    }
    // IPv4-mapped IPv6 (e.g. ::ffff:127.0.0.1)
    if let Some(ipv4_part) = h.strip_prefix("::ffff:") {
        if is_blocked_ipv4(ipv4_part) {
            return Err(format!("blocked host (private/reserved): {host}"));
        }
    }
    // Unique local, link-local
    if h.starts_with("fc") || h.starts_with("fd") || h.starts_with("fe80") {
        return Err(format!("blocked host (private/link-local): {host}"));
    }
    // IPv4-compatible IPv6 (::a.b.c.d)
    if h.starts_with("::") && !h.starts_with("::ffff:") && h.len() > 2 {
        let rest = &h[2..];
        if is_blocked_ipv4(rest) {
            return Err(format!("blocked host (private/reserved): {host}"));
        }
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

    #[test]
    fn blocks_cgnat() {
        assert!(ensure_public_http_url("http://100.64.0.1/").is_err());
    }

    #[test]
    fn blocks_test_nets() {
        assert!(ensure_public_http_url("http://192.0.2.1/").is_err());
        assert!(ensure_public_http_url("http://198.51.100.1/").is_err());
        assert!(ensure_public_http_url("http://203.0.113.1/").is_err());
    }

    #[test]
    fn blocks_multicast_and_reserved() {
        assert!(ensure_public_http_url("http://224.0.0.1/").is_err());
        assert!(ensure_public_http_url("http://240.0.0.1/").is_err());
    }

    #[test]
    fn blocks_ipv4_mapped_ipv6_loopback() {
        assert!(ensure_public_http_url("http://[::ffff:127.0.0.1]/").is_err());
    }

    #[test]
    fn blocks_ipv4_mapped_ipv6_private() {
        assert!(ensure_public_http_url("http://[::ffff:10.0.0.1]/").is_err());
        assert!(ensure_public_http_url("http://[::ffff:192.168.1.1]/").is_err());
    }
}
