// Adapted from vibelang-dsp (https://github.com/trusch/vibelang), MIT OR Apache-2.0.
//
// SynthDef v2 binary encoder: turns a GraphIR into the exact bytes
// scsynth expects for `/d_recv` / `/d_load`, replacing what used to be
// sclang's `SynthDef(...).writeDefFile(...)`.

use super::errors::*;
use super::graph::*;
use byteorder::{BigEndian, WriteBytesExt};
use std::io::Write;

/// Encode a GraphIR into a SynthDef v2 binary format.
pub fn encode_synthdef(ir: &GraphIR) -> Result<Vec<u8>> {
    ir.validate()?;

    let mut buf = Vec::new();

    buf.write_all(b"SCgf")
        .map_err(|e| SynthDefError::EncodingError(format!("failed to write header: {}", e)))?;
    buf.write_i32::<BigEndian>(2)
        .map_err(|e| SynthDefError::EncodingError(format!("failed to write version: {}", e)))?;
    buf.write_i16::<BigEndian>(1)
        .map_err(|e| SynthDefError::EncodingError(format!("failed to write def count: {}", e)))?;

    encode_graph(&mut buf, ir)?;

    Ok(buf)
}

fn encode_graph(buf: &mut Vec<u8>, ir: &GraphIR) -> Result<()> {
    write_pstring(buf, &ir.name)?;

    buf.write_i32::<BigEndian>(ir.constants.len() as i32)
        .map_err(|e| {
            SynthDefError::EncodingError(format!("failed to write constant count: {}", e))
        })?;
    for &c in &ir.constants {
        buf.write_f32::<BigEndian>(c).map_err(|e| {
            SynthDefError::EncodingError(format!("failed to write constant: {}", e))
        })?;
    }

    let total_slots = ir.total_param_slots();
    buf.write_i32::<BigEndian>(total_slots as i32).map_err(|e| {
        SynthDefError::EncodingError(format!("failed to write param count: {}", e))
    })?;

    let mut param_values = vec![0.0f32; total_slots];
    for param in &ir.params {
        for (i, &val) in param.default.iter().enumerate() {
            param_values[param.index + i] = val;
        }
    }
    for &val in &param_values {
        buf.write_f32::<BigEndian>(val).map_err(|e| {
            SynthDefError::EncodingError(format!("failed to write param default: {}", e))
        })?;
    }

    buf.write_i32::<BigEndian>(ir.params.len() as i32).map_err(|e| {
        SynthDefError::EncodingError(format!("failed to write param name count: {}", e))
    })?;
    for param in &ir.params {
        write_pstring(buf, &param.name)?;
        buf.write_i32::<BigEndian>(param.index as i32).map_err(|e| {
            SynthDefError::EncodingError(format!("failed to write param name index: {}", e))
        })?;
    }

    buf.write_i32::<BigEndian>(ir.nodes.len() as i32)
        .map_err(|e| SynthDefError::EncodingError(format!("failed to write ugen count: {}", e)))?;
    for node in &ir.nodes {
        encode_ugen(buf, node, &ir.constants)?;
    }

    buf.write_i16::<BigEndian>(0).map_err(|e| {
        SynthDefError::EncodingError(format!("failed to write variant count: {}", e))
    })?;

    Ok(())
}

fn encode_ugen(buf: &mut Vec<u8>, node: &UGenNode, constants: &[f32]) -> Result<()> {
    write_pstring(buf, &node.name)?;

    buf.write_i8(node.rate.as_byte() as i8)
        .map_err(|e| SynthDefError::EncodingError(format!("failed to write ugen rate: {}", e)))?;
    buf.write_i32::<BigEndian>(node.inputs.len() as i32).map_err(|e| {
        SynthDefError::EncodingError(format!("failed to write input count: {}", e))
    })?;
    buf.write_i32::<BigEndian>(node.num_outputs as i32).map_err(|e| {
        SynthDefError::EncodingError(format!("failed to write output count: {}", e))
    })?;
    buf.write_i16::<BigEndian>(node.special_index).map_err(|e| {
        SynthDefError::EncodingError(format!("failed to write special index: {}", e))
    })?;

    for input in &node.inputs {
        match input {
            Input::Constant(val) => {
                buf.write_i32::<BigEndian>(-1).map_err(|e| {
                    SynthDefError::EncodingError(format!("failed to write constant input: {}", e))
                })?;
                let const_idx = constants
                    .iter()
                    .position(|&c| (c - val).abs() < 1e-9)
                    .ok_or_else(|| {
                        SynthDefError::EncodingError(format!("constant {} not found in table", val))
                    })?;
                buf.write_i32::<BigEndian>(const_idx as i32).map_err(|e| {
                    SynthDefError::EncodingError(format!("failed to write constant index: {}", e))
                })?;
            }
            Input::Node {
                node_id,
                output_index,
            } => {
                buf.write_i32::<BigEndian>(*node_id as i32).map_err(|e| {
                    SynthDefError::EncodingError(format!("failed to write node input: {}", e))
                })?;
                buf.write_i32::<BigEndian>(*output_index as i32).map_err(|e| {
                    SynthDefError::EncodingError(format!("failed to write output index: {}", e))
                })?;
            }
        }
    }

    for _ in 0..node.num_outputs {
        buf.write_i8(node.rate.as_byte() as i8).map_err(|e| {
            SynthDefError::EncodingError(format!("failed to write output rate: {}", e))
        })?;
    }

    Ok(())
}

fn write_pstring(buf: &mut Vec<u8>, s: &str) -> Result<()> {
    let bytes = s.as_bytes();
    if bytes.len() > 255 {
        return Err(SynthDefError::EncodingError(format!(
            "string too long for pstring: {}",
            s
        )));
    }
    buf.write_u8(bytes.len() as u8).map_err(|e| {
        SynthDefError::EncodingError(format!("failed to write string length: {}", e))
    })?;
    buf.write_all(bytes)
        .map_err(|e| SynthDefError::EncodingError(format!("failed to write string: {}", e)))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_encode_empty_synthdef() {
        let builder = GraphBuilderInner::new();
        let ir = GraphIR::from_builder("empty".to_string(), builder);
        let bytes = encode_synthdef(&ir).unwrap();
        assert_eq!(&bytes[0..4], b"SCgf");
        assert_eq!(bytes[4..8], [0, 0, 0, 2]);
    }

    #[test]
    fn test_pstring_encoding() {
        let mut buf = Vec::new();
        write_pstring(&mut buf, "test").unwrap();
        assert_eq!(buf, vec![4, b't', b'e', b's', b't']);
    }
}
