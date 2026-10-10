use serde::{Deserialize, Serialize};
use std::collections::HashMap;

pub const INTERACTIVE_ROLES: &[&str] = &[
    "button",
    "link",
    "textbox",
    "searchbox",
    "checkbox",
    "combobox",
    "menuitem",
    "tab",
    "radio",
    "switch",
    "slider",
];

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AxNode {
    pub node_id: String,
    pub backend_dom_node_id: Option<u64>,
    pub role: String,
    pub name: Option<String>,
    pub value: Option<String>,
    pub description: Option<String>,
    pub disabled: bool,
    pub child_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct InteractiveElement {
    pub reference_id: String,
    pub backend_dom_node_id: u64,
    pub role: String,
    pub name: String,
    pub value: Option<String>,
}

pub struct AxTreeSnapshot {
    pub nodes: HashMap<String, AxNode>,
    pub root_id: Option<String>,
    pub ref_map: HashMap<String, u64>,
}

impl AxTreeSnapshot {
    pub fn from_nodes(nodes: Vec<AxNode>) -> Self {
        let mut map = HashMap::new();
        let mut root_id = None;

        for (i, node) in nodes.into_iter().enumerate() {
            if i == 0 {
                root_id = Some(node.node_id.clone());
            }
            map.insert(node.node_id.clone(), node);
        }

        Self {
            nodes: map,
            root_id,
            ref_map: HashMap::new(),
        }
    }

    /// Extracts interactive elements and generates stable `ref-xxxxxx` IDs
    pub fn extract_interactive_elements(&mut self) -> Vec<InteractiveElement> {
        let mut results = Vec::new();
        let mut ref_counter = 100000u64;

        for node in self.nodes.values() {
            if node.disabled {
                continue;
            }

            let is_interactive = INTERACTIVE_ROLES.contains(&node.role.as_str());
            if is_interactive {
                if let Some(backend_id) = node.backend_dom_node_id {
                    let ref_id = format!("ref-{:x}", ref_counter);
                    ref_counter += 1;

                    self.ref_map.insert(ref_id.clone(), backend_id);

                    results.push(InteractiveElement {
                        reference_id: ref_id,
                        backend_dom_node_id: backend_id,
                        role: node.role.clone(),
                        name: node.name.clone().unwrap_or_default(),
                        value: node.value.clone(),
                    });
                }
            }
        }

        results
    }

    /// Formats the interactive snapshot as a clean, minimal text representation
    pub fn format_interactive_list(&mut self) -> String {
        let elements = self.extract_interactive_elements();
        let mut out = String::new();

        for el in elements {
            out.push_str(&format!(
                "- [{}] {} \"{}\"",
                el.reference_id, el.role, el.name
            ));
            if let Some(val) = el.value {
                out.push_str(&format!(" (value: \"{}\")", val));
            }
            out.push('\n');
        }

        out
    }
}
