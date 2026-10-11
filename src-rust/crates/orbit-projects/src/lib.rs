use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProjectKnowledgeItem {
    pub id: Uuid,
    pub path: String,
    pub title: String,
    pub content: String,
}

impl ProjectKnowledgeItem {
    pub fn new(
        path: impl Into<String>,
        title: impl Into<String>,
        content: impl Into<String>,
    ) -> Self {
        Self {
            id: Uuid::new_v4(),
            path: path.into(),
            title: title.into(),
            content: content.into(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct OrbitProject {
    pub id: Uuid,
    pub name: String,
    pub description: String,
    pub custom_instructions: Option<String>,
    pub knowledge_items: Vec<ProjectKnowledgeItem>,
    pub pinned_rules: Vec<String>,
}

impl OrbitProject {
    pub fn new(name: impl Into<String>, description: impl Into<String>) -> Self {
        Self {
            id: Uuid::new_v4(),
            name: name.into(),
            description: description.into(),
            custom_instructions: None,
            knowledge_items: Vec::new(),
            pinned_rules: Vec::new(),
        }
    }

    pub fn with_instructions(mut self, instructions: impl Into<String>) -> Self {
        self.custom_instructions = Some(instructions.into());
        self
    }

    pub fn add_knowledge(&mut self, item: ProjectKnowledgeItem) {
        self.knowledge_items.push(item);
    }

    /// Generates prompt context matching Claude Desktop Project Knowledge and Cursor Rules
    pub fn format_prompt_context(&self) -> String {
        let mut out = String::new();
        out.push_str("<project_context>\n");
        out.push_str(&format!("<project_name>{}</project_name>\n", self.name));

        if !self.description.is_empty() {
            out.push_str(&format!(
                "<project_description>{}</project_description>\n",
                self.description
            ));
        }

        if let Some(ref inst) = self.custom_instructions {
            if !inst.trim().is_empty() {
                out.push_str("<project_custom_instructions>\n");
                out.push_str(inst.trim());
                out.push_str("\n</project_custom_instructions>\n");
            }
        }

        if !self.knowledge_items.is_empty() {
            out.push_str("<project_knowledge_base>\n");
            for item in &self.knowledge_items {
                out.push_str(&format!(
                    "<document path=\"{}\" title=\"{}\">\n",
                    item.path, item.title
                ));
                out.push_str(item.content.trim());
                out.push_str("\n</document>\n");
            }
            out.push_str("</project_knowledge_base>\n");
        }

        if !self.pinned_rules.is_empty() {
            out.push_str("<pinned_project_rules>\n");
            for rule in &self.pinned_rules {
                out.push_str(&format!("- {}\n", rule));
            }
            out.push_str("</pinned_project_rules>\n");
        }

        out.push_str("</project_context>");
        out
    }
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct ProjectStore {
    pub active_project_id: Option<Uuid>,
    pub projects: HashMap<Uuid, OrbitProject>,
}

impl ProjectStore {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add_project(&mut self, project: OrbitProject) -> Uuid {
        let id = project.id;
        self.projects.insert(id, project);
        if self.active_project_id.is_none() {
            self.active_project_id = Some(id);
        }
        id
    }

    pub fn get_active_project(&self) -> Option<&OrbitProject> {
        self.active_project_id.and_then(|id| self.projects.get(&id))
    }

    pub fn set_active_project(&mut self, id: Uuid) -> bool {
        if self.projects.contains_key(&id) {
            self.active_project_id = Some(id);
            true
        } else {
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_project_prompt_formatting() {
        let mut proj = OrbitProject::new("Nelo Studio", "Global novel engine")
            .with_instructions("Strictly use purple branding and avoid emojis.");

        proj.add_knowledge(ProjectKnowledgeItem::new(
            "docs/spec.md",
            "Specification",
            "# Specification\nAll endpoints require Bearer auth.",
        ));

        let formatted = proj.format_prompt_context();
        assert!(formatted.contains("<project_name>Nelo Studio</project_name>"));
        assert!(formatted.contains("Strictly use purple branding"));
        assert!(formatted.contains("<document path=\"docs/spec.md\" title=\"Specification\">"));
        assert!(formatted.contains("All endpoints require Bearer auth."));
    }

    #[test]
    fn test_project_store_lifecycle() {
        let mut store = ProjectStore::new();
        assert!(store.get_active_project().is_none());

        let p1 = OrbitProject::new("Alpha", "First project");
        let _id1 = store.add_project(p1);
        assert_eq!(store.get_active_project().unwrap().name, "Alpha");

        let p2 = OrbitProject::new("Beta", "Second project");
        let id2 = store.add_project(p2);
        assert_eq!(store.get_active_project().unwrap().name, "Alpha");

        assert!(store.set_active_project(id2));
        assert_eq!(store.get_active_project().unwrap().name, "Beta");
    }
}
