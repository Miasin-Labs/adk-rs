use std::collections::{BTreeSet, VecDeque};

use crate::workflow::{WorkflowError, WorkflowGraph};

#[derive(Debug, Clone)]
pub struct WorkflowRuntime {
    graph: WorkflowGraph,
}

impl WorkflowRuntime {
    pub fn new(graph: WorkflowGraph) -> Self {
        Self { graph }
    }

    pub fn run_from_roots(&self) -> Result<Vec<String>, WorkflowRuntimeError> {
        let mut queue = self
            .graph
            .roots()
            .into_iter()
            .map(|node| node.id.clone())
            .collect::<VecDeque<_>>();
        let mut visited = BTreeSet::new();
        let mut order = Vec::new();

        while let Some(node_id) = queue.pop_front() {
            if !visited.insert(node_id.clone()) {
                continue;
            }
            if !self.graph.nodes.contains_key(&node_id) {
                return Err(WorkflowRuntimeError::Graph(WorkflowError::UnknownNode(
                    node_id,
                )));
            }
            order.push(node_id.clone());
            queue.extend(
                self.graph
                    .next_nodes(&node_id)
                    .into_iter()
                    .map(|node| node.id.clone()),
            );
        }

        Ok(order)
    }
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum WorkflowRuntimeError {
    #[error(transparent)]
    Graph(#[from] WorkflowError),
}
