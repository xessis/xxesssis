use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet, VecDeque};
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CanvasNodeKind {
    Agent,
    Task,
    Diff,
    Todo,
    Chart,
    Callout,
    Terminal,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CanvasNodeStatus {
    Idle,
    Running,
    Success,
    Failed,
    AwaitingApproval,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CanvasPosition {
    pub x: f64,
    pub y: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CanvasDimensions {
    pub width: f64,
    pub height: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CanvasNode {
    pub id: Uuid,
    pub label: String,
    pub kind: CanvasNodeKind,
    pub status: CanvasNodeStatus,
    pub position: CanvasPosition,
    pub dimensions: CanvasDimensions,
    pub metadata: HashMap<String, serde_json::Value>,
}

impl CanvasNode {
    pub fn new(label: impl Into<String>, kind: CanvasNodeKind) -> Self {
        Self {
            id: Uuid::new_v4(),
            label: label.into(),
            kind,
            status: CanvasNodeStatus::Idle,
            position: CanvasPosition { x: 0.0, y: 0.0 },
            dimensions: CanvasDimensions {
                width: 280.0,
                height: 120.0,
            },
            metadata: HashMap::new(),
        }
    }

    pub fn with_position(mut self, x: f64, y: f64) -> Self {
        self.position = CanvasPosition { x, y };
        self
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum EdgeStyle {
    Solid,
    Dashed,
    Animated,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CanvasEdge {
    pub id: Uuid,
    pub source: Uuid,
    pub target: Uuid,
    pub label: Option<String>,
    pub style: EdgeStyle,
}

impl CanvasEdge {
    pub fn new(source: Uuid, target: Uuid) -> Self {
        Self {
            id: Uuid::new_v4(),
            source,
            target,
            label: None,
            style: EdgeStyle::Solid,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TodoItem {
    pub id: Uuid,
    pub text: String,
    pub completed: bool,
}

impl TodoItem {
    pub fn new(text: impl Into<String>) -> Self {
        Self {
            id: Uuid::new_v4(),
            text: text.into(),
            completed: false,
        }
    }
}

pub struct CanvasSession {
    pub nodes: HashMap<Uuid, CanvasNode>,
    pub edges: Vec<CanvasEdge>,
}

impl Default for CanvasSession {
    fn default() -> Self {
        Self::new()
    }
}

impl CanvasSession {
    pub fn new() -> Self {
        Self {
            nodes: HashMap::new(),
            edges: Vec::new(),
        }
    }

    pub fn add_node(&mut self, node: CanvasNode) -> Uuid {
        let id = node.id;
        self.nodes.insert(id, node);
        id
    }

    pub fn connect(&mut self, source: Uuid, target: Uuid) {
        self.edges.push(CanvasEdge::new(source, target));
    }

    /// Automatically computes a hierarchical DAG layout
    pub fn compute_dag_layout(&mut self, horizontal_spacing: f64, vertical_spacing: f64) {
        let mut in_degrees: HashMap<Uuid, usize> = HashMap::new();
        let mut adj: HashMap<Uuid, Vec<Uuid>> = HashMap::new();

        for &id in self.nodes.keys() {
            in_degrees.insert(id, 0);
            adj.insert(id, Vec::new());
        }

        for edge in &self.edges {
            *in_degrees.entry(edge.target).or_insert(0) += 1;
            adj.entry(edge.source).or_default().push(edge.target);
        }

        // Topological layer assignment
        let mut queue: VecDeque<(Uuid, usize)> = VecDeque::new();
        for (&id, &deg) in &in_degrees {
            if deg == 0 {
                queue.push_back((id, 0));
            }
        }

        let mut node_layers: HashMap<Uuid, usize> = HashMap::new();
        let mut layers: HashMap<usize, Vec<Uuid>> = HashMap::new();
        let mut visited: HashSet<Uuid> = HashSet::new();

        while let Some((node_id, layer)) = queue.pop_front() {
            if !visited.insert(node_id) {
                continue;
            }
            node_layers.insert(node_id, layer);
            layers.entry(layer).or_default().push(node_id);

            if let Some(neighbors) = adj.get(&node_id) {
                for &next in neighbors {
                    queue.push_back((next, layer + 1));
                }
            }
        }

        // Apply calculated layout coordinates
        for (layer_idx, node_ids) in layers {
            let count = node_ids.len() as f64;
            let total_height = count * 140.0 + (count - 1.0) * vertical_spacing;
            let start_y = -total_height / 2.0;

            for (idx, node_id) in node_ids.into_iter().enumerate() {
                if let Some(node) = self.nodes.get_mut(&node_id) {
                    node.position.x = layer_idx as f64 * (280.0 + horizontal_spacing);
                    node.position.y = start_y + (idx as f64 * (140.0 + vertical_spacing));
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_canvas_dag_layout() {
        let mut session = CanvasSession::new();
        let n1 = session.add_node(CanvasNode::new("Planner", CanvasNodeKind::Agent));
        let n2 = session.add_node(CanvasNode::new("Coder", CanvasNodeKind::Agent));
        let n3 = session.add_node(CanvasNode::new("Tester", CanvasNodeKind::Agent));

        session.connect(n1, n2);
        session.connect(n2, n3);

        session.compute_dag_layout(60.0, 40.0);

        let p1 = &session.nodes[&n1].position;
        let p2 = &session.nodes[&n2].position;
        let p3 = &session.nodes[&n3].position;

        assert!(p1.x < p2.x);
        assert!(p2.x < p3.x);
    }

    #[test]
    fn test_todo_items() {
        let mut item = TodoItem::new("Audit codebase");
        assert!(!item.completed);
        item.completed = true;
        assert!(item.completed);
    }
}
