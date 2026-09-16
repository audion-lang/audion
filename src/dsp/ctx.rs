// Small helper layer on top of `graph::GraphBuilderInner` used by
// `synthdef.rs` to build a UGen graph directly (replacing the old
// sclang-text codegen).
//
// BinaryOpUGen / UnaryOpUGen selector indices below are SuperCollider's
// fixed, version-stable operator tables (server/plugins/{Binary,Unary}OpUGens.cpp
// in the supercollider source tree) — NOT reused from vibelang-dsp, whose
// rhainodes.rs has at least one wrong index (it uses special_index 28 for
// `tanh`, which is actually `sin`; the real `tanh` selector is 36).

use super::graph::{GraphBuilderInner, Input, Rate};

pub mod binop {
    pub const ADD: i16 = 0;
    pub const SUB: i16 = 1;
    pub const MUL: i16 = 2;
    pub const DIV: i16 = 4;
    pub const MOD: i16 = 5;
    pub const EQ: i16 = 6;
    pub const NE: i16 = 7;
    pub const LT: i16 = 8;
    pub const GT: i16 = 9;
    pub const LE: i16 = 10;
    pub const GE: i16 = 11;
    pub const MIN: i16 = 12;
    pub const MAX: i16 = 13;
    pub const BITAND: i16 = 14;
    pub const BITOR: i16 = 15;
    pub const BITXOR: i16 = 16;
    pub const SHIFT_LEFT: i16 = 26;
    pub const SHIFT_RIGHT: i16 = 27;
    pub const POW: i16 = 25;
    pub const CLIP2: i16 = 42;
}

pub mod unop {
    pub const ABS: i16 = 5;
    pub const CEIL: i16 = 8;
    pub const FLOOR: i16 = 9;
    pub const SIGN: i16 = 11;
    pub const SQUARED: i16 = 12;
    pub const CUBED: i16 = 13;
    pub const SQRT: i16 = 14;
    pub const EXP: i16 = 15;
    pub const LOG: i16 = 25;
    pub const LOG2: i16 = 26;
    pub const ARC_TAN: i16 = 33;
    pub const TANH: i16 = 36;
    pub const DISTORT: i16 = 42;
    pub const SOFT_CLIP: i16 = 43;
}

/// A bare constant input. Free function (not a `BuildCtx` method) so it can
/// be nested inside another `ctx.*(...)` call's argument list without
/// tripping the borrow checker — `GraphIR::from_builder` collects the
/// final deduplicated constants table by scanning node inputs, so no
/// bookkeeping is needed here.
pub fn konst(v: f32) -> Input {
    Input::Constant(v)
}

/// Build context: wraps the graph builder with convenience helpers used
/// throughout the UGen-call translation in `synthdef.rs`.
pub struct BuildCtx {
    pub builder: GraphBuilderInner,
}

impl BuildCtx {
    pub fn new() -> Self {
        Self {
            builder: GraphBuilderInner::new(),
        }
    }

    pub fn konst(&mut self, v: f32) -> Input {
        konst(v)
    }

    /// Add a node, returning one `Input` per output (channel-expanded).
    pub fn node(
        &mut self,
        name: &str,
        rate: Rate,
        inputs: Vec<Input>,
        num_outputs: u32,
        special_index: i16,
    ) -> Vec<Input> {
        let id = self
            .builder
            .add_node(name.to_string(), rate, inputs, num_outputs, special_index);
        (0..num_outputs)
            .map(|o| Input::Node {
                node_id: id,
                output_index: o,
            })
            .collect()
    }

    /// Add a single-output node, returning its one `Input`.
    pub fn node1(&mut self, name: &str, rate: Rate, inputs: Vec<Input>, special_index: i16) -> Input {
        self.node(name, rate, inputs, 1, special_index)[0]
    }

    pub fn rate_of(&self, inputs: &[Input]) -> Rate {
        self.builder.max_rate_from_inputs(inputs)
    }

    pub fn binop(&mut self, op: i16, a: Input, b: Input) -> Input {
        let rate = self.rate_of(&[a, b]);
        self.node1("BinaryOpUGen", rate, vec![a, b], op)
    }

    pub fn unop(&mut self, op: i16, a: Input) -> Input {
        let rate = self.rate_of(&[a]);
        self.node1("UnaryOpUGen", rate, vec![a], op)
    }
}

impl Default for BuildCtx {
    fn default() -> Self {
        Self::new()
    }
}

/// Get `args[idx]` or a constant default, used by the (already
/// channel-expanded, so always-scalar) UGen-call arms.
pub fn arg_or(args: &[Input], idx: usize, default: f32) -> Input {
    args.get(idx).copied().unwrap_or(Input::Constant(default))
}
