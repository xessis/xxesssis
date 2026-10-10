use std::net::IpAddr;
use url::Url;

pub const ALLOWED_CDP_METHODS: &[&str] = &[
    "Accessibility.getFullAXTree",
    "CSS.enable",
    "CSS.disable",
    "CSS.getComputedStyleForNode",
    "DOM.getDocument",
    "DOM.describeNode",
    "DOM.getBoxModel",
    "DOM.querySelector",
    "DOM.querySelectorAll",
    "DOM.focus",
    "Page.enable",
    "Page.getLayoutMetrics",
    "Performance.getMetrics",
    "Log.enable",
    "Runtime.enable",
];

pub const DENIED_CDP_PREFIXES: &[&str] = &[
    "Input.",
    "Network.getCookies",
    "Network.getResponseBody",
    "Storage.",
    "IndexedDB.",
    "CacheStorage.",
    "Page.navigate",
    "DOM.setFileInputFiles",
    "Target.",
];

pub fn is_cdp_method_allowed(method: &str) -> bool {
    let trimmed = method.trim();
    for denied in DENIED_CDP_PREFIXES {
        if trimmed.starts_with(denied) {
            return false;
        }
    }
    ALLOWED_CDP_METHODS.contains(&trimmed)
}

pub fn is_eval_script_safe(script: &str) -> bool {
    let lower = script.to_lowercase();
    let forbidden_patterns = [
        "document.cookie",
        "localstorage",
        "sessionstorage",
        "indexeddb",
        "openwindow",
        "eval(",
    ];

    for pattern in forbidden_patterns {
        if lower.contains(pattern) {
            return false;
        }
    }
    true
}

pub fn is_blocked_ip(ip: &IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => {
            let octets = v4.octets();
            // Block 169.254.0.0/16 (Link Local & AWS/GCP/Azure Metadata)
            octets[0] == 169 && octets[1] == 254
        }
        IpAddr::V6(v6) => {
            // Check for IPv4-mapped IPv6 address (::ffff:a.b.c.d)
            if let Some(v4) = v6.to_ipv4_mapped() {
                return is_blocked_ip(&IpAddr::V4(v4));
            }

            let seg0 = v6.segments()[0];
            // Block fe80::/10 (Link-Local)
            let is_link_local = (seg0 & 0xffc0) == 0xfe80;
            // Block fc00::/7 (Unique Local, e.g. AWS IMDSv6 fd00:ec2::254)
            let is_unique_local = (seg0 & 0xfe00) == 0xfc00;

            is_link_local || is_unique_local
        }
    }
}

pub fn is_navigable_url(target: &str) -> bool {
    if target == "about:blank" {
        return true;
    }

    let parsed = match Url::parse(target) {
        Ok(u) => u,
        Err(_) => return false,
    };

    let scheme = parsed.scheme();
    if scheme != "http" && scheme != "https" {
        return false;
    }

    if parsed.username() != "" || parsed.password().is_some() {
        return false;
    }

    let host = match parsed.host_str() {
        Some(h) => h,
        None => return false,
    };

    if host.eq_ignore_ascii_case("metadata.google.internal") {
        return false;
    }

    if let Ok(ip) = host.parse::<IpAddr>() {
        return !is_blocked_ip(&ip);
    }

    true
}
