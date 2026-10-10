pub mod axtree;
pub mod security;
pub mod typing;

pub use axtree::{AxNode, AxTreeSnapshot, InteractiveElement, INTERACTIVE_ROLES};
pub use security::{
    is_blocked_ip, is_cdp_method_allowed, is_eval_script_safe, is_navigable_url,
    ALLOWED_CDP_METHODS, DENIED_CDP_PREFIXES,
};
pub use typing::{TypingAction, TypingTier};

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::IpAddr;

    #[test]
    fn test_ssrf_filter_blocks_metadata_and_link_local() {
        let aws_metadata: IpAddr = "169.254.169.254".parse().unwrap();
        assert!(is_blocked_ip(&aws_metadata));

        let ipv6_link_local: IpAddr = "fe80::1".parse().unwrap();
        assert!(is_blocked_ip(&ipv6_link_local));

        let ipv6_unique_local: IpAddr = "fc00::1".parse().unwrap();
        assert!(is_blocked_ip(&ipv6_unique_local));

        let aws_imds_v6: IpAddr = "fd00:ec2::254".parse().unwrap();
        assert!(is_blocked_ip(&aws_imds_v6));

        let ipv4_mapped: IpAddr = "::ffff:169.254.169.254".parse().unwrap();
        assert!(is_blocked_ip(&ipv4_mapped));

        let public_ip: IpAddr = "8.8.8.8".parse().unwrap();
        assert!(!is_blocked_ip(&public_ip));
    }

    #[test]
    fn test_navigable_url_validation() {
        assert!(is_navigable_url("https://example.com"));
        assert!(is_navigable_url("http://localhost:3000"));
        assert!(!is_navigable_url(
            "http://169.254.169.254/latest/meta-data/"
        ));
        assert!(!is_navigable_url(
            "http://metadata.google.internal/computeMetadata/v1/"
        ));
        assert!(!is_navigable_url("file:///etc/passwd"));
        assert!(!is_navigable_url("javascript:alert(1)"));
        assert!(!is_navigable_url("http://admin:pass@example.com"));
    }

    #[test]
    fn test_cdp_method_allowlist_and_denylist() {
        assert!(is_cdp_method_allowed("Accessibility.getFullAXTree"));
        assert!(is_cdp_method_allowed("DOM.getBoxModel"));
        assert!(!is_cdp_method_allowed("Input.dispatchKeyEvent"));
        assert!(!is_cdp_method_allowed("Network.getCookies"));
        assert!(!is_cdp_method_allowed("Storage.clearDataForOrigin"));
        assert!(!is_cdp_method_allowed("Target.createTarget"));
    }

    #[test]
    fn test_eval_script_safety() {
        assert!(is_eval_script_safe(
            "document.querySelector('h1').innerText"
        ));
        assert!(!is_eval_script_safe("console.log(document.cookie)"));
        assert!(!is_eval_script_safe("window.localStorage.getItem('token')"));
    }

    #[test]
    fn test_axtree_interactive_snapshot_extraction() {
        let node1 = AxNode {
            node_id: "1".to_string(),
            backend_dom_node_id: Some(42),
            role: "button".to_string(),
            name: Some("Submit Order".to_string()),
            value: None,
            description: None,
            disabled: false,
            child_ids: vec![],
        };

        let node2 = AxNode {
            node_id: "2".to_string(),
            backend_dom_node_id: Some(43),
            role: "generic".to_string(),
            name: None,
            value: None,
            description: None,
            disabled: false,
            child_ids: vec![],
        };

        let node3 = AxNode {
            node_id: "3".to_string(),
            backend_dom_node_id: Some(44),
            role: "textbox".to_string(),
            name: Some("Search".to_string()),
            value: Some("query text".to_string()),
            description: None,
            disabled: false,
            child_ids: vec![],
        };

        let mut tree = AxTreeSnapshot::from_nodes(vec![node1, node2, node3]);
        let formatted = tree.format_interactive_list();

        assert!(formatted.contains("Submit Order"));
        assert!(formatted.contains("textbox \"Search\" (value: \"query text\")"));
        assert!(!formatted.contains("generic"));
        assert_eq!(tree.ref_map.len(), 2);
    }
}
