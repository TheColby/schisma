//! Versioned, typed graph document used by Schisma's constrained topology editor.

use schisma_params::ParamId;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use thiserror::Error;

pub const GRAPH_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct NodeId(pub u64);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum NodeScope {
    PerVoice,
    Global,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PortKind {
    AudioMono,
    AudioStereo,
    Event,
    Control,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum NodeKind {
    Wavetable,
    Exciter,
    Morph,
    ModalBody,
    Filter,
    Drive,
    Delay,
    VoiceBus,
    Reverb,
    Equalizer,
    Limiter,
    Output,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GraphNode {
    pub id: NodeId,
    pub kind: NodeKind,
    pub name: String,
    pub scope: NodeScope,
    pub position: [f32; 2],
    pub inputs: Vec<PortKind>,
    pub outputs: Vec<PortKind>,
    pub parameters: BTreeMap<ParamId, f64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Connection {
    pub from_node: NodeId,
    pub from_port: usize,
    pub to_node: NodeId,
    pub to_port: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GraphDocument {
    pub schema_version: u32,
    pub name: String,
    pub nodes: Vec<GraphNode>,
    pub connections: Vec<Connection>,
}

impl GraphDocument {
    pub fn validate(&self) -> Result<(), GraphError> {
        if self.schema_version != GRAPH_SCHEMA_VERSION {
            return Err(GraphError::UnsupportedSchema(self.schema_version));
        }

        let mut ids = BTreeSet::new();
        let mut by_id = BTreeMap::new();
        for node in &self.nodes {
            if !ids.insert(node.id) {
                return Err(GraphError::DuplicateNode(node.id));
            }
            by_id.insert(node.id, node);
        }

        for connection in &self.connections {
            let source = by_id
                .get(&connection.from_node)
                .ok_or(GraphError::MissingNode(connection.from_node))?;
            let target = by_id
                .get(&connection.to_node)
                .ok_or(GraphError::MissingNode(connection.to_node))?;
            let source_kind =
                source
                    .outputs
                    .get(connection.from_port)
                    .ok_or(GraphError::MissingOutput(
                        connection.from_node,
                        connection.from_port,
                    ))?;
            let target_kind =
                target
                    .inputs
                    .get(connection.to_port)
                    .ok_or(GraphError::MissingInput(
                        connection.to_node,
                        connection.to_port,
                    ))?;
            if source_kind != target_kind {
                return Err(GraphError::PortTypeMismatch {
                    from: *source_kind,
                    to: *target_kind,
                });
            }
            if source.scope == NodeScope::Global && target.scope == NodeScope::PerVoice {
                return Err(GraphError::GlobalAudioIntoVoice);
            }
            if source.scope == NodeScope::PerVoice
                && target.scope == NodeScope::Global
                && target.kind != NodeKind::VoiceBus
            {
                return Err(GraphError::MissingVoiceBus);
            }
        }

        self.validate_acyclic(&by_id)
    }

    fn validate_acyclic(&self, by_id: &BTreeMap<NodeId, &GraphNode>) -> Result<(), GraphError> {
        let mut indegree = BTreeMap::<NodeId, usize>::new();
        let mut outgoing = BTreeMap::<NodeId, Vec<NodeId>>::new();
        for node in &self.nodes {
            indegree.insert(node.id, 0);
        }
        for edge in &self.connections {
            let source = by_id[&edge.from_node];
            if source.kind == NodeKind::Delay {
                continue;
            }
            outgoing
                .entry(edge.from_node)
                .or_default()
                .push(edge.to_node);
            *indegree.entry(edge.to_node).or_default() += 1;
        }

        let mut ready: Vec<NodeId> = indegree
            .iter()
            .filter_map(|(id, degree)| (*degree == 0).then_some(*id))
            .collect();
        let mut visited = 0;
        while let Some(id) = ready.pop() {
            visited += 1;
            for target in outgoing.get(&id).into_iter().flatten() {
                let degree = indegree.get_mut(target).expect("validated node ID");
                *degree -= 1;
                if *degree == 0 {
                    ready.push(*target);
                }
            }
        }
        if visited == self.nodes.len() {
            Ok(())
        } else {
            Err(GraphError::FeedbackWithoutDelay)
        }
    }
}

pub fn default_instrument_graph() -> GraphDocument {
    let nodes = vec![
        node(
            1,
            NodeKind::Wavetable,
            "Wavetable",
            NodeScope::PerVoice,
            [40.0, 100.0],
            vec![],
            vec![PortKind::AudioMono],
        ),
        node(
            2,
            NodeKind::Morph,
            "Energy Morph",
            NodeScope::PerVoice,
            [250.0, 100.0],
            vec![PortKind::AudioMono],
            vec![PortKind::AudioMono],
        ),
        node(
            3,
            NodeKind::ModalBody,
            "Modal Body",
            NodeScope::PerVoice,
            [470.0, 100.0],
            vec![PortKind::AudioMono],
            vec![PortKind::AudioMono],
        ),
        node(
            4,
            NodeKind::Filter,
            "TPT Filter",
            NodeScope::PerVoice,
            [690.0, 100.0],
            vec![PortKind::AudioMono],
            vec![PortKind::AudioStereo],
        ),
        node(
            5,
            NodeKind::VoiceBus,
            "Voice Bus ×16",
            NodeScope::Global,
            [910.0, 100.0],
            vec![PortKind::AudioStereo],
            vec![PortKind::AudioStereo],
        ),
        node(
            6,
            NodeKind::Limiter,
            "Safety Limiter",
            NodeScope::Global,
            [1130.0, 100.0],
            vec![PortKind::AudioStereo],
            vec![PortKind::AudioStereo],
        ),
        node(
            7,
            NodeKind::Output,
            "Stereo Output",
            NodeScope::Global,
            [1350.0, 100.0],
            vec![PortKind::AudioStereo],
            vec![],
        ),
    ];
    let connections = (1..=6)
        .map(|id| Connection {
            from_node: NodeId(id),
            from_port: 0,
            to_node: NodeId(id + 1),
            to_port: 0,
        })
        .collect();
    GraphDocument {
        schema_version: GRAPH_SCHEMA_VERSION,
        name: "Schisma v0.1 Default".into(),
        nodes,
        connections,
    }
}

fn node(
    id: u64,
    kind: NodeKind,
    name: &str,
    scope: NodeScope,
    position: [f32; 2],
    inputs: Vec<PortKind>,
    outputs: Vec<PortKind>,
) -> GraphNode {
    GraphNode {
        id: NodeId(id),
        kind,
        name: name.into(),
        scope,
        position,
        inputs,
        outputs,
        parameters: BTreeMap::new(),
    }
}

#[derive(Debug, Error, PartialEq)]
pub enum GraphError {
    #[error("unsupported graph schema {0}")]
    UnsupportedSchema(u32),
    #[error("duplicate node {0:?}")]
    DuplicateNode(NodeId),
    #[error("missing node {0:?}")]
    MissingNode(NodeId),
    #[error("node {0:?} has no output {1}")]
    MissingOutput(NodeId, usize),
    #[error("node {0:?} has no input {1}")]
    MissingInput(NodeId, usize),
    #[error("port type mismatch from {from:?} to {to:?}")]
    PortTypeMismatch { from: PortKind, to: PortKind },
    #[error("global audio cannot flow into a per-voice node")]
    GlobalAudioIntoVoice,
    #[error("per-voice audio must enter the global graph through VoiceBus")]
    MissingVoiceBus,
    #[error("feedback requires an explicit Delay node")]
    FeedbackWithoutDelay,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_graph_is_valid() {
        default_instrument_graph().validate().unwrap();
    }

    #[test]
    fn feedback_requires_delay() {
        let mut graph = default_instrument_graph();
        graph.connections.push(Connection {
            from_node: NodeId(3),
            from_port: 0,
            to_node: NodeId(2),
            to_port: 0,
        });
        assert_eq!(graph.validate(), Err(GraphError::FeedbackWithoutDelay));
    }
}
