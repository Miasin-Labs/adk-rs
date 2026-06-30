use std::collections::{BTreeMap, BTreeSet, VecDeque};

use crate::workflow::{WorkflowError, WorkflowGraph};

/// A handler that can execute a workflow node.
///
/// Implementations should take the node ID and input string, perform some operation,
/// and return an output string.
pub trait NodeHandler: Send + Sync {
    /// Handle execution of a node.
    ///
    /// # Arguments
    /// * `node_id` - The unique identifier of the node being executed
    /// * `input` - The input string (empty for root nodes or concatenated upstream outputs)
    ///
    /// # Returns
    /// A result containing the output string or an error
    fn handle(&self, node_id: &str, input: &str) -> Result<String, WorkflowRuntimeError>;
}

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

    /// Execute the workflow starting from root nodes using the provided handlers.
    ///
    /// # Arguments
    /// * `handlers` - A map from node ID to handler implementation
    /// * `initial_input` - The input string for root nodes
    ///
    /// # Returns
    /// A vector of (node_id, output) tuples in execution order, or an error if execution fails
    pub fn execute(
        &self,
        handlers: &BTreeMap<String, Box<dyn NodeHandler>>,
        initial_input: &str,
    ) -> Result<Vec<(String, String)>, WorkflowRuntimeError> {
        // Get the execution order using BFS
        let execution_order = self.run_from_roots()?;

        // Track outputs for each node
        let mut node_outputs: BTreeMap<String, String> = BTreeMap::new();
        let mut results = Vec::new();

        for node_id in execution_order {
            // Get upstream inputs from predecessor nodes
            let input = self.collect_input(&node_id, &node_outputs, initial_input)?;

            // Find and call the handler
            let handler = handlers
                .get(&node_id)
                .ok_or_else(|| WorkflowRuntimeError::MissingHandler(node_id.clone()))?;

            let output = handler.handle(&node_id, &input)?;

            // Store the output for downstream nodes
            node_outputs.insert(node_id.clone(), output.clone());
            results.push((node_id, output));
        }

        Ok(results)
    }

    /// Collect input for a node from its predecessors.
    ///
    /// For nodes with multiple predecessors (Join nodes), outputs are concatenated with newlines.
    fn collect_input(
        &self,
        node_id: &str,
        node_outputs: &BTreeMap<String, String>,
        initial_input: &str,
    ) -> Result<String, WorkflowRuntimeError> {
        // Find all edges that point to this node
        let predecessors: Vec<&str> = self
            .graph
            .edges
            .iter()
            .filter(|edge| edge.to == node_id)
            .map(|edge| edge.from.as_str())
            .collect();

        if predecessors.is_empty() {
            // Root node: use initial input
            Ok(initial_input.to_string())
        } else {
            // Collect outputs from all predecessors, concatenated with newlines
            let inputs: Result<Vec<_>, _> = predecessors
                .iter()
                .map(|pred| {
                    node_outputs.get(*pred).cloned().ok_or_else(|| {
                        WorkflowRuntimeError::Graph(WorkflowError::UnknownNode(pred.to_string()))
                    })
                })
                .collect();

            inputs.map(|inputs| inputs.join("\n"))
        }
    }
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum WorkflowRuntimeError {
    #[error(transparent)]
    Graph(#[from] WorkflowError),
    #[error("missing handler for node {0}")]
    MissingHandler(String),
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::workflow::{WorkflowEdge, WorkflowNode, WorkflowNodeKind};
    use std::sync::Mutex;

    /// A simple test handler that appends the node ID to the input
    struct AppendNodeIdHandler;

    impl NodeHandler for AppendNodeIdHandler {
        fn handle(&self, node_id: &str, input: &str) -> Result<String, WorkflowRuntimeError> {
            if input.is_empty() {
                Ok(node_id.to_string())
            } else {
                Ok(format!("{}/{}", input, node_id))
            }
        }
    }

    /// A handler that transforms input using a custom function
    struct TransformHandler<F>
    where
        F: Fn(&str) -> String + Send + Sync,
    {
        transform: F,
    }

    impl<F> NodeHandler for TransformHandler<F>
    where
        F: Fn(&str) -> String + Send + Sync,
    {
        fn handle(&self, _node_id: &str, input: &str) -> Result<String, WorkflowRuntimeError> {
            Ok((self.transform)(input))
        }
    }

    #[test]
    fn test_workflow_simple_linear_execution() {
        // Build graph: A -> B -> C
        let mut graph = WorkflowGraph::default();

        let node_a = WorkflowNode {
            id: "A".to_string(),
            kind: WorkflowNodeKind::Agent("agent_a".to_string()),
        };
        let node_b = WorkflowNode {
            id: "B".to_string(),
            kind: WorkflowNodeKind::Function("func_b".to_string()),
        };
        let node_c = WorkflowNode {
            id: "C".to_string(),
            kind: WorkflowNodeKind::Tool("tool_c".to_string()),
        };

        graph.add_node(node_a);
        graph.add_node(node_b);
        graph.add_node(node_c);

        graph.add_edge(WorkflowEdge {
            from: "A".to_string(),
            to: "B".to_string(),
            route: None,
        }).unwrap();

        graph.add_edge(WorkflowEdge {
            from: "B".to_string(),
            to: "C".to_string(),
            route: None,
        }).unwrap();

        let runtime = WorkflowRuntime::new(graph);

        let mut handlers: BTreeMap<String, Box<dyn NodeHandler>> = BTreeMap::new();
        handlers.insert("A".to_string(), Box::new(AppendNodeIdHandler));
        handlers.insert("B".to_string(), Box::new(AppendNodeIdHandler));
        handlers.insert("C".to_string(), Box::new(AppendNodeIdHandler));

        let results = runtime
            .execute(&handlers, "")
            .expect("execution should succeed");

        assert_eq!(results.len(), 3);
        assert_eq!(results[0], ("A".to_string(), "A".to_string()));
        assert_eq!(results[1], ("B".to_string(), "A/B".to_string()));
        assert_eq!(results[2], ("C".to_string(), "A/B/C".to_string()));
    }

    #[test]
    fn test_workflow_execution_order_bfs() {
        // Build graph: Root with two children that converge
        //     A
        //    / \
        //   B   C
        //    \ /
        //     D
        let mut graph = WorkflowGraph::default();

        graph.add_node(WorkflowNode {
            id: "A".to_string(),
            kind: WorkflowNodeKind::Agent("agent".to_string()),
        });
        graph.add_node(WorkflowNode {
            id: "B".to_string(),
            kind: WorkflowNodeKind::Function("func".to_string()),
        });
        graph.add_node(WorkflowNode {
            id: "C".to_string(),
            kind: WorkflowNodeKind::Function("func".to_string()),
        });
        graph.add_node(WorkflowNode {
            id: "D".to_string(),
            kind: WorkflowNodeKind::Join,
        });

        graph.add_edge(WorkflowEdge {
            from: "A".to_string(),
            to: "B".to_string(),
            route: None,
        }).unwrap();
        graph.add_edge(WorkflowEdge {
            from: "A".to_string(),
            to: "C".to_string(),
            route: None,
        }).unwrap();
        graph.add_edge(WorkflowEdge {
            from: "B".to_string(),
            to: "D".to_string(),
            route: None,
        }).unwrap();
        graph.add_edge(WorkflowEdge {
            from: "C".to_string(),
            to: "D".to_string(),
            route: None,
        }).unwrap();

        let runtime = WorkflowRuntime::new(graph);

        let mut handlers: BTreeMap<String, Box<dyn NodeHandler>> = BTreeMap::new();
        handlers.insert("A".to_string(), Box::new(AppendNodeIdHandler));
        handlers.insert("B".to_string(), Box::new(AppendNodeIdHandler));
        handlers.insert("C".to_string(), Box::new(AppendNodeIdHandler));
        handlers.insert("D".to_string(), Box::new(AppendNodeIdHandler));

        let results = runtime
            .execute(&handlers, "")
            .expect("execution should succeed");

        assert_eq!(results.len(), 4);
        // A is root
        assert_eq!(results[0].0, "A");
        assert_eq!(results[0].1, "A");

        // B and C should execute after A (order may vary within level)
        assert!(results[1].0 == "B" || results[1].0 == "C");
        assert!(results[2].0 == "B" || results[2].0 == "C");

        // D should execute last
        assert_eq!(results[3].0, "D");
        // D receives concatenated inputs from B and C
        let d_input = &results[3].1;
        assert!(d_input.contains("A/B") && d_input.contains("A/C"));
    }

    #[test]
    fn test_workflow_execution_with_initial_input() {
        // Build graph: A -> B
        let mut graph = WorkflowGraph::default();

        graph.add_node(WorkflowNode {
            id: "A".to_string(),
            kind: WorkflowNodeKind::Agent("agent".to_string()),
        });
        graph.add_node(WorkflowNode {
            id: "B".to_string(),
            kind: WorkflowNodeKind::Function("func".to_string()),
        });

        graph.add_edge(WorkflowEdge {
            from: "A".to_string(),
            to: "B".to_string(),
            route: None,
        }).unwrap();

        let runtime = WorkflowRuntime::new(graph);

        let mut handlers: BTreeMap<String, Box<dyn NodeHandler>> = BTreeMap::new();
        handlers.insert("A".to_string(), Box::new(AppendNodeIdHandler));
        handlers.insert("B".to_string(), Box::new(AppendNodeIdHandler));

        let initial = "START";
        let results = runtime
            .execute(&handlers, initial)
            .expect("execution should succeed");

        assert_eq!(results.len(), 2);
        assert_eq!(results[0], ("A".to_string(), "START/A".to_string()));
        assert_eq!(results[1], ("B".to_string(), "START/A/B".to_string()));
    }

    #[test]
    fn test_workflow_execution_missing_handler() {
        // Build graph: A -> B
        let mut graph = WorkflowGraph::default();

        graph.add_node(WorkflowNode {
            id: "A".to_string(),
            kind: WorkflowNodeKind::Agent("agent".to_string()),
        });
        graph.add_node(WorkflowNode {
            id: "B".to_string(),
            kind: WorkflowNodeKind::Function("func".to_string()),
        });

        graph.add_edge(WorkflowEdge {
            from: "A".to_string(),
            to: "B".to_string(),
            route: None,
        }).unwrap();

        let runtime = WorkflowRuntime::new(graph);

        let mut handlers: BTreeMap<String, Box<dyn NodeHandler>> = BTreeMap::new();
        // Only register handler for A, not B
        handlers.insert("A".to_string(), Box::new(AppendNodeIdHandler));

        let result = runtime.execute(&handlers, "");

        assert!(result.is_err());
        match result.unwrap_err() {
            WorkflowRuntimeError::MissingHandler(node_id) => {
                assert_eq!(node_id, "B");
            }
            _ => panic!("Expected MissingHandler error"),
        }
    }

    #[test]
    fn test_workflow_run_from_roots_unchanged() {
        // Verify that run_from_roots still works as expected
        let mut graph = WorkflowGraph::default();

        graph.add_node(WorkflowNode {
            id: "A".to_string(),
            kind: WorkflowNodeKind::Agent("agent".to_string()),
        });
        graph.add_node(WorkflowNode {
            id: "B".to_string(),
            kind: WorkflowNodeKind::Function("func".to_string()),
        });
        graph.add_node(WorkflowNode {
            id: "C".to_string(),
            kind: WorkflowNodeKind::Tool("tool".to_string()),
        });

        graph.add_edge(WorkflowEdge {
            from: "A".to_string(),
            to: "B".to_string(),
            route: None,
        }).unwrap();
        graph.add_edge(WorkflowEdge {
            from: "B".to_string(),
            to: "C".to_string(),
            route: None,
        }).unwrap();

        let runtime = WorkflowRuntime::new(graph);
        let order = runtime.run_from_roots().expect("should return order");

        assert_eq!(order, vec!["A", "B", "C"]);
    }
}
