use url::Url;

pub fn normalize_url_input(input: &str) -> Result<String, String> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Err("navigation target is empty".to_string());
    }

    if is_special_scheme(trimmed) {
        return Ok(trimmed.to_string());
    }

    if !trimmed.contains("://") && (is_localhost_target(trimmed) || trimmed.contains(':')) {
        let candidate = format!("http://{trimmed}");
        return Url::parse(&candidate)
            .map(|url| url.to_string())
            .map_err(|error| format!("invalid URL: {error}"));
    }

    if let Ok(parsed) = Url::parse(trimmed) {
        let scheme = parsed.scheme();
        if scheme == "http" || scheme == "https" {
            return Ok(parsed.to_string());
        }
        if !scheme.is_empty() {
            return Err(format!("unsupported URL scheme: {scheme}"));
        }
    }

    if trimmed.contains("://") {
        return Err("invalid URL".to_string());
    }

    let candidate = if is_localhost_target(trimmed) || trimmed.contains(':') {
        format!("http://{trimmed}")
    } else {
        format!("https://{trimmed}")
    };

    Url::parse(&candidate)
        .map(|url| url.to_string())
        .map_err(|error| format!("invalid URL: {error}"))
}

pub fn resolve_startup_url(value: &str) -> Result<Option<String>, String> {
    let trimmed = value.trim();
    if trimmed.is_empty() || trimmed == "about:blank" {
        return Ok(None);
    }
    normalize_url_input(trimmed).map(Some)
}

fn is_special_scheme(value: &str) -> bool {
    value.starts_with("about:") || value.starts_with("data:") || value.starts_with("file:")
}

fn is_localhost_target(value: &str) -> bool {
    value.starts_with("localhost") || value.starts_with("127.") || value.starts_with("0.0.0.0")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_bare_domains() {
        let url = normalize_url_input("example.com").unwrap();
        assert_eq!(url, "https://example.com/");
    }

    #[test]
    fn preserves_about_blank() {
        let url = normalize_url_input("about:blank").unwrap();
        assert_eq!(url, "about:blank");
    }

    #[test]
    fn prefixes_localhost_with_http() {
        let url = normalize_url_input("localhost:8000").unwrap();
        assert_eq!(url, "http://localhost:8000/");
    }

    #[test]
    fn rejects_unknown_scheme() {
        let error = normalize_url_input("chrome://version").unwrap_err();
        assert!(error.contains("unsupported"));
    }

    #[test]
    fn resolve_startup_url_skips_blank() {
        assert_eq!(resolve_startup_url("about:blank").unwrap(), None);
        assert_eq!(resolve_startup_url("").unwrap(), None);
    }
}
