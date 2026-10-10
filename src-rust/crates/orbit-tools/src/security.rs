use orbit_common::{OrbitError, Result};
use std::path::{Component, Path};

pub fn assert_no_traversal(path: impl AsRef<Path>) -> Result<()> {
    for component in path.as_ref().components() {
        if component == Component::ParentDir {
            return Err(OrbitError::Security(
                "Path traversal (..) is strictly forbidden".to_string(),
            ));
        }
    }
    Ok(())
}

pub fn is_inside_workspace(workspace_root: impl AsRef<Path>, target: impl AsRef<Path>) -> bool {
    let root = match workspace_root.as_ref().canonicalize() {
        Ok(p) => p,
        Err(_) => return false,
    };

    let target_path = target.as_ref();
    if let Ok(tgt) = target_path.canonicalize() {
        return tgt.starts_with(&root);
    }

    // Target does not exist yet: canonicalize closest existing ancestor
    let mut current = target_path;
    let mut relative_suffix = Vec::new();
    while !current.exists() {
        if let Some(file_name) = current.file_name() {
            relative_suffix.push(file_name);
        }
        if let Some(parent) = current.parent() {
            if parent == current || parent.as_os_str().is_empty() {
                break;
            }
            current = parent;
        } else {
            break;
        }
    }

    if let Ok(mut canonical_ancestor) = current.canonicalize() {
        for comp in relative_suffix.into_iter().rev() {
            canonical_ancestor.push(comp);
        }
        canonical_ancestor.starts_with(&root)
    } else {
        false
    }
}

pub fn is_sensitive_path(path: impl AsRef<Path>) -> bool {
    let p = path.as_ref();
    let path_str = p.to_string_lossy();
    let lower_str = path_str.to_lowercase();

    // Check system files
    if lower_str.contains("/etc/shadow")
        || lower_str.contains("/etc/passwd")
        || lower_str.ends_with("/sam")
        || lower_str.ends_with("\\sam")
    {
        return true;
    }

    // Check filenames and exact extensions
    if let Some(file_name) = p.file_name().and_then(|n| n.to_str()) {
        let lower_name = file_name.to_lowercase();
        if lower_name.starts_with(".env")
            || lower_name == ".netrc"
            || lower_name == ".pgpass"
            || lower_name == ".htpasswd"
            || lower_name == ".npmrc"
            || lower_name == "token.txt"
            || lower_name.starts_with("id_rsa")
            || lower_name.starts_with("id_ecdsa")
            || lower_name.starts_with("id_ed25519")
            || lower_name.starts_with("id_dsa")
            || lower_name.contains("credentials")
            || lower_name == ".bash_history"
        {
            return true;
        }
    }

    // Check directory components
    for component in p.components() {
        if let Component::Normal(comp) = component {
            let comp_str = comp.to_string_lossy().to_lowercase();
            if comp_str == ".ssh"
                || comp_str == ".aws"
                || comp_str == ".gnupg"
                || comp_str == ".kube"
                || comp_str == "secrets"
            {
                return true;
            }
        }
    }

    // Check sensitive file extensions
    if let Some(ext) = p.extension().and_then(|e| e.to_str()) {
        let lower_ext = ext.to_lowercase();
        if lower_ext == "pem"
            || lower_ext == "key"
            || lower_ext == "p12"
            || lower_ext == "pfx"
            || lower_ext == "keystore"
        {
            return true;
        }
    }

    false
}
