use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct WorkflowNode {
    pub id: String,
    pub kind: WorkflowNodeKind,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum WorkflowNodeKind {
    Agent(String),
    Function(String),
    Tool(String),
    Join,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkflowEdge {
    pub from: String,
    pub to: String,
    pub route: Option<String>,
}

#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkflowGraph {
    pub nodes: BTreeMap<String, WorkflowNode>,
    pub edges: Vec<WorkflowEdge>,
}

impl WorkflowGraph {
    pub fn add_node(&mut self, node: WorkflowNode) {
        self.nodes.insert(node.id.clone(), node);
    }

    pub fn add_edge(&mut self, edge: WorkflowEdge) -> Result<(), WorkflowError> {
        if !self.nodes.contains_key(&edge.from) {
            return Err(WorkflowError::UnknownNode(edge.from));
        }
        if !self.nodes.contains_key(&edge.to) {
            return Err(WorkflowError::UnknownNode(edge.to));
        }
        self.edges.push(edge);
        Ok(())
    }

    pub fn next_nodes(&self, node_id: &str) -> Vec<&WorkflowNode> {
        self.edges
            .iter()
            .filter(|edge| edge.from == node_id)
            .filter_map(|edge| self.nodes.get(&edge.to))
            .collect()
    }

    pub fn roots(&self) -> Vec<&WorkflowNode> {
        let targets = self
            .edges
            .iter()
            .map(|edge| edge.to.as_str())
            .collect::<BTreeSet<_>>();
        self.nodes
            .iter()
            .filter(|(id, _)| !targets.contains(id.as_str()))
            .map(|(_, node)| node)
            .collect()
    }
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum WorkflowError {
    #[error("unknown workflow node {0}")]
    UnknownNode(String),
}
