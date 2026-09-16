// Adapted from vibelang-dsp (https://github.com/trusch/vibelang), MIT OR Apache-2.0.
// Graph IR for UGen graphs: nodes, inputs, parameters, and the mutable
// builder used while walking a `define` body. Ported near-verbatim; only
// the Rhai-facing bits were dropped since audion drives this straight from
// Rust (no scripting layer in between).

use super::errors::*;
use std::collections::HashMap;

/// Rate of a UGen (audio, control, scalar). Ordering: Scalar < Control < Audio.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Rate {
    Scalar = 0,
    Control = 1,
    Audio = 2,
}

impl Rate {
    pub fn as_byte(&self) -> u8 {
        *self as u8
    }
}

/// Input to a UGen - either a constant or another node's output.
#[derive(Clone, Copy, Debug)]
pub enum Input {
    Constant(f32),
    Node { node_id: u32, output_index: u32 },
}

/// A UGen node in the graph.
#[derive(Clone, Debug)]
pub struct UGenNode {
    pub name: String,
    pub rate: Rate,
    pub inputs: Vec<Input>,
    pub num_outputs: u32,
    pub special_index: i16,
}

/// Parameter specification.
#[derive(Clone, Debug)]
pub struct ParamSpec {
    pub name: String,
    pub default: Vec<f32>,
    pub index: usize,
}

/// The mutable state of a graph builder, accumulated while walking a
/// `define` body.
pub struct GraphBuilderInner {
    pub nodes: Vec<UGenNode>,
    pub constants: Vec<f32>,
    pub params: Vec<ParamSpec>,
    pub param_map: HashMap<String, u32>,
}

impl GraphBuilderInner {
    pub fn new() -> Self {
        Self {
            nodes: Vec::new(),
            constants: Vec::new(),
            params: Vec::new(),
            param_map: HashMap::new(),
        }
    }

    /// Add a constant to the graph, returns its index. Constants are
    /// deduplicated - if the value already exists, its existing index is
    /// returned.
    pub fn add_constant(&mut self, value: f32) -> usize {
        for (i, &c) in self.constants.iter().enumerate() {
            if (c - value).abs() < 1e-9 {
                return i;
            }
        }
        let idx = self.constants.len();
        self.constants.push(value);
        idx
    }

    /// Add a UGen node, returns its node id.
    pub fn add_node(
        &mut self,
        name: String,
        rate: Rate,
        inputs: Vec<Input>,
        num_outputs: u32,
        special_index: i16,
    ) -> u32 {
        let id = self.nodes.len() as u32;
        self.nodes.push(UGenNode {
            name,
            rate,
            inputs,
            num_outputs,
            special_index,
        });
        id
    }

    pub fn add_param(&mut self, name: String, default: Vec<f32>) -> u32 {
        let id = self.params.len() as u32;
        let index = self.total_param_slots();
        self.params.push(ParamSpec {
            name: name.clone(),
            default,
            index,
        });
        self.param_map.insert(name, id);
        id
    }

    pub fn total_param_slots(&self) -> usize {
        self.params.iter().map(|p| p.default.len()).sum()
    }

    pub fn get_node_rate(&self, node_id: u32) -> Rate {
        self.nodes
            .get(node_id as usize)
            .map(|n| n.rate)
            .unwrap_or(Rate::Scalar)
    }

    /// Compute the maximum rate from a list of inputs - used to determine
    /// the rate of BinaryOpUGen/UnaryOpUGen nodes.
    pub fn max_rate_from_inputs(&self, inputs: &[Input]) -> Rate {
        let mut max_rate = Rate::Scalar;
        for input in inputs {
            let input_rate = match input {
                Input::Constant(_) => Rate::Scalar,
                Input::Node { node_id, .. } => self.get_node_rate(*node_id),
            };
            if input_rate > max_rate {
                max_rate = input_rate;
            }
        }
        max_rate
    }

    /// Create the Control UGen node for parameters. Must be called after
    /// all parameters are added, before any other nodes.
    pub fn create_control_ugen(&mut self) {
        if self.params.is_empty() {
            return;
        }
        let total_slots = self.total_param_slots() as u32;
        let control_node = UGenNode {
            name: "Control".to_string(),
            rate: Rate::Control,
            inputs: Vec::new(),
            num_outputs: total_slots,
            special_index: 0,
        };
        self.nodes.insert(0, control_node);
    }

    fn ensure_max_local_bufs(&mut self) {
        let local_buf_count = self.nodes.iter().filter(|n| n.name == "LocalBuf").count();
        if local_buf_count == 0 || self.nodes.iter().any(|n| n.name == "MaxLocalBufs") {
            return;
        }
        let insert_at = if self.nodes.first().is_some_and(|n| n.name == "Control") {
            1
        } else {
            0
        };
        self.add_constant(local_buf_count as f32);
        self.shift_node_references_for_insert(insert_at);
        self.nodes.insert(
            insert_at,
            UGenNode {
                name: "MaxLocalBufs".to_string(),
                rate: Rate::Scalar,
                inputs: vec![Input::Constant(local_buf_count as f32)],
                num_outputs: 1,
                special_index: 0,
            },
        );
    }

    fn shift_node_references_for_insert(&mut self, insert_at: usize) {
        for node in &mut self.nodes {
            for input in &mut node.inputs {
                if let Input::Node { node_id, .. } = input {
                    if *node_id as usize >= insert_at {
                        *node_id += 1;
                    }
                }
            }
        }
    }
}

impl Default for GraphBuilderInner {
    fn default() -> Self {
        Self::new()
    }
}

/// Final graph IR ready for encoding.
#[derive(Clone, Debug)]
pub struct GraphIR {
    pub name: String,
    pub constants: Vec<f32>,
    pub params: Vec<ParamSpec>,
    pub nodes: Vec<UGenNode>,
}

impl GraphIR {
    /// Builds the final constants table by scanning every node's inputs
    /// for `Input::Constant` values, deduplicated. Nodes are built with
    /// bare `Input::Constant(v)` values (no separate constant-table
    /// bookkeeping needed while walking the `define` body — see
    /// `dsp::ctx::konst`), so this is the one place constants get collected.
    pub fn from_builder(name: String, mut builder: GraphBuilderInner) -> Self {
        builder.ensure_max_local_bufs();
        let mut constants: Vec<f32> = Vec::new();
        for node in &builder.nodes {
            for input in &node.inputs {
                if let Input::Constant(v) = input {
                    if !constants.iter().any(|&c: &f32| (c - v).abs() < 1e-9) {
                        constants.push(*v);
                    }
                }
            }
        }
        Self {
            name,
            constants,
            params: builder.params,
            nodes: builder.nodes,
        }
    }

    pub fn total_param_slots(&self) -> usize {
        self.params.iter().map(|p| p.default.len()).sum()
    }

    /// Validate the graph structure before encoding.
    pub fn validate(&self) -> Result<()> {
        if !self.params.is_empty() {
            if self.nodes.is_empty() {
                return Err(SynthDefError::ValidationError(
                    "graph has parameters but no Control UGen".to_string(),
                ));
            }
            let control_node = &self.nodes[0];
            if control_node.name != "Control" {
                return Err(SynthDefError::ValidationError(format!(
                    "first UGen should be Control, got {}",
                    control_node.name
                )));
            }
            if control_node.rate != Rate::Control {
                return Err(SynthDefError::ValidationError(
                    "Control UGen must have Control rate".to_string(),
                ));
            }
            let expected_outputs = self.total_param_slots() as u32;
            if control_node.num_outputs != expected_outputs {
                return Err(SynthDefError::ValidationError(format!(
                    "Control UGen has {} outputs, expected {}",
                    control_node.num_outputs, expected_outputs
                )));
            }
        }

        for (idx, node) in self.nodes.iter().enumerate() {
            for input in &node.inputs {
                if let Input::Node { node_id, .. } = input {
                    if *node_id >= idx as u32 {
                        return Err(SynthDefError::ValidationError(format!(
                            "UGen {} references future UGen {} - violates topological order",
                            idx, node_id
                        )));
                    }
                }
            }
        }

        let total_slots = self.total_param_slots();
        if total_slots > 0 {
            let mut covered = vec![false; total_slots];
            for param in &self.params {
                for i in 0..param.default.len() {
                    let slot = param.index + i;
                    if slot >= total_slots {
                        return Err(SynthDefError::ValidationError(format!(
                            "parameter '{}' index {} exceeds total slots {}",
                            param.name, slot, total_slots
                        )));
                    }
                    covered[slot] = true;
                }
            }
            for (i, &is_covered) in covered.iter().enumerate() {
                if !is_covered {
                    return Err(SynthDefError::ValidationError(format!(
                        "parameter slot {} is not covered by any parameter",
                        i
                    )));
                }
            }
        }

        for (idx, node) in self.nodes.iter().enumerate() {
            for input in &node.inputs {
                if let Input::Node {
                    node_id,
                    output_index,
                } = input
                {
                    if *node_id as usize >= self.nodes.len() {
                        return Err(SynthDefError::ValidationError(format!(
                            "UGen {} references invalid node {}",
                            idx, node_id
                        )));
                    }
                    let referenced_node = &self.nodes[*node_id as usize];
                    if *output_index >= referenced_node.num_outputs {
                        return Err(SynthDefError::ValidationError(format!(
                            "UGen {} references output {} of node {}, but it only has {} outputs",
                            idx, output_index, node_id, referenced_node.num_outputs
                        )));
                    }
                }
            }
        }

        Ok(())
    }
}
