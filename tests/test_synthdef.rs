// These used to assert on generated sclang source text; `synthdef.rs` now
// builds and encodes a SynthDef binary directly (see `src/dsp/`), so these
// parse the resulting `scsyndef` bytes instead of matching source strings.

use audion::ast::{BinOp, UGenExpr};
use audion::synthdef::{build_synthdef, collect_sample_paths, BufferInfo};

fn na() -> Vec<(String, UGenExpr)> {
    vec![]
}

// --- minimal scsyndef v2 reader -------------------------------------------

struct Reader<'a> {
    b: &'a [u8],
    pos: usize,
}

#[derive(Debug)]
struct ParamInfo {
    name: String,
    index: i32,
}

#[derive(Debug)]
struct UGenInfo {
    name: String,
    rate: i8,
    num_inputs: i32,
    num_outputs: i32,
    special_index: i16,
    /// (node_id, output_index) for a Node input, or None for a Constant.
    inputs: Vec<Option<(i32, i32)>>,
}

struct ParsedDef {
    name: String,
    constants: Vec<f32>,
    param_defaults: Vec<f32>,
    params: Vec<ParamInfo>,
    ugens: Vec<UGenInfo>,
}

impl<'a> Reader<'a> {
    fn new(b: &'a [u8]) -> Self {
        Self { b, pos: 0 }
    }
    fn u8(&mut self) -> u8 {
        let v = self.b[self.pos];
        self.pos += 1;
        v
    }
    fn i8(&mut self) -> i8 {
        self.u8() as i8
    }
    fn i16(&mut self) -> i16 {
        let v = i16::from_be_bytes([self.b[self.pos], self.b[self.pos + 1]]);
        self.pos += 2;
        v
    }
    fn i32(&mut self) -> i32 {
        let v = i32::from_be_bytes([
            self.b[self.pos],
            self.b[self.pos + 1],
            self.b[self.pos + 2],
            self.b[self.pos + 3],
        ]);
        self.pos += 4;
        v
    }
    fn f32(&mut self) -> f32 {
        let v = f32::from_be_bytes([
            self.b[self.pos],
            self.b[self.pos + 1],
            self.b[self.pos + 2],
            self.b[self.pos + 3],
        ]);
        self.pos += 4;
        v
    }
    fn pstring(&mut self) -> String {
        let len = self.u8() as usize;
        let s = String::from_utf8_lossy(&self.b[self.pos..self.pos + len]).to_string();
        self.pos += len;
        s
    }
}

fn parse_synthdef(bytes: &[u8]) -> ParsedDef {
    let mut r = Reader::new(bytes);
    assert_eq!(&bytes[0..4], b"SCgf", "missing SCgf header");
    r.pos = 4;
    let version = r.i32();
    assert_eq!(version, 2);
    let num_defs = r.i16();
    assert_eq!(num_defs, 1, "test helper only parses single-def files");

    let name = r.pstring();

    let const_count = r.i32();
    let mut constants = Vec::new();
    for _ in 0..const_count {
        constants.push(r.f32());
    }

    let param_count = r.i32();
    let mut param_defaults = Vec::new();
    for _ in 0..param_count {
        param_defaults.push(r.f32());
    }

    let param_name_count = r.i32();
    let mut params = Vec::new();
    for _ in 0..param_name_count {
        let name = r.pstring();
        let index = r.i32();
        params.push(ParamInfo { name, index });
    }

    let ugen_count = r.i32();
    let mut ugens = Vec::new();
    for _ in 0..ugen_count {
        let name = r.pstring();
        let rate = r.i8();
        let num_inputs = r.i32();
        let num_outputs = r.i32();
        let special_index = r.i16();
        let mut inputs = Vec::new();
        for _ in 0..num_inputs {
            let a = r.i32();
            let b = r.i32();
            inputs.push(if a == -1 { None } else { Some((a, b)) });
        }
        for _ in 0..num_outputs {
            r.i8();
        }
        ugens.push(UGenInfo {
            name,
            rate,
            num_inputs,
            num_outputs,
            special_index,
            inputs,
        });
    }

    let _variant_count = r.i16();
    ParsedDef {
        name,
        constants,
        param_defaults,
        params,
        ugens,
    }
}

fn ugen_names(def: &ParsedDef) -> Vec<&str> {
    def.ugens.iter().map(|u| u.name.as_str()).collect()
}

#[test]
fn test_simple_sine() {
    let body = UGenExpr::UGenCall {
        name: "out".to_string(),
        args: vec![
            UGenExpr::Number(0.0),
            UGenExpr::UGenCall {
                name: "sine".to_string(),
                args: vec![UGenExpr::Param("freq".to_string())],
                named_args: na(),
            },
        ],
        named_args: na(),
    };
    let bytes = build_synthdef("test_sine", &["freq".to_string()], &body, &[]).unwrap();
    let def = parse_synthdef(&bytes);
    assert_eq!(def.name, "test_sine");
    let names = ugen_names(&def);
    assert!(names.contains(&"Control"));
    assert!(names.contains(&"SinOsc"));
    assert!(names.contains(&"Out"));
    let sine = def.ugens.iter().find(|u| u.name == "SinOsc").unwrap();
    assert_eq!(sine.rate, 2, "sine() should be audio-rate");
}

#[test]
fn test_filtered_saw() {
    let body = UGenExpr::UGenCall {
        name: "out".to_string(),
        args: vec![
            UGenExpr::Number(0.0),
            UGenExpr::UGenCall {
                name: "lpf".to_string(),
                args: vec![
                    UGenExpr::UGenCall {
                        name: "saw".to_string(),
                        args: vec![UGenExpr::Param("freq".to_string())],
                        named_args: na(),
                    },
                    UGenExpr::Number(2000.0),
                ],
                named_args: na(),
            },
        ],
        named_args: na(),
    };
    let bytes = build_synthdef("test_saw", &["freq".to_string()], &body, &[]).unwrap();
    let def = parse_synthdef(&bytes);
    let names = ugen_names(&def);
    assert!(names.contains(&"Saw"));
    assert!(names.contains(&"LPF"));
    assert!(def.constants.contains(&2000.0));
    let lpf = def.ugens.iter().find(|u| u.name == "LPF").unwrap();
    // LPF(in, freq): first input is the Saw node, second the 2000.0 constant.
    assert_eq!(lpf.num_inputs, 2);
    assert!(lpf.inputs[1].is_none(), "second LPF input should be the constant 2000.0");
}

#[test]
fn test_with_envelope() {
    let body = UGenExpr::UGenCall {
        name: "out".to_string(),
        args: vec![
            UGenExpr::Number(0.0),
            UGenExpr::BinOp {
                left: Box::new(UGenExpr::UGenCall {
                    name: "saw".to_string(),
                    args: vec![UGenExpr::Param("freq".to_string())],
                    named_args: na(),
                }),
                op: BinOp::Mul,
                right: Box::new(UGenExpr::UGenCall {
                    name: "env".to_string(),
                    args: vec![UGenExpr::Param("gate".to_string())],
                    named_args: na(),
                }),
            },
        ],
        named_args: na(),
    };
    let bytes = build_synthdef(
        "test_env",
        &["freq".to_string(), "gate".to_string()],
        &body,
        &[],
    )
    .unwrap();
    let def = parse_synthdef(&bytes);
    let names = ugen_names(&def);
    assert!(names.contains(&"EnvGen"));
    assert!(names.contains(&"BinaryOpUGen"), "saw * env should be a BinaryOpUGen mul");
    let env_gen = def.ugens.iter().find(|u| u.name == "EnvGen").unwrap();
    // [gate, levelScale, levelBias, timeScale, doneAction, initLevel, numSegments,
    //  releaseNode, loopNode, level, dur, shape, curve] * 2 segments = 9 + 8 = 17
    assert_eq!(env_gen.num_inputs, 17);
    // default env(gate) atk=0.01, sus=1(sustainLevel), rel=0.3 -> Env.asr levels [0,1,0]
    assert!(def.constants.contains(&0.01));
    assert!(def.constants.contains(&0.3));
}

#[test]
fn test_param_defaults() {
    let body = UGenExpr::UGenCall {
        name: "out".to_string(),
        args: vec![
            UGenExpr::Param("out".to_string()),
            UGenExpr::UGenCall {
                name: "sine".to_string(),
                args: vec![UGenExpr::Param("freq".to_string())],
                named_args: na(),
            },
        ],
        named_args: na(),
    };
    let bytes = build_synthdef(
        "test_defaults",
        &["freq".to_string(), "amp".to_string(), "out".to_string()],
        &body,
        &[],
    )
    .unwrap();
    let def = parse_synthdef(&bytes);

    let get = |n: &str| -> f32 {
        let p = def.params.iter().find(|p| p.name == n).unwrap();
        def.param_defaults[p.index as usize]
    };
    assert_eq!(get("freq"), 440.0);
    assert_eq!(get("amp"), 0.1);
    assert_eq!(get("out"), 0.0);
}

#[test]
fn test_sample_ugen() {
    let body = UGenExpr::UGenCall {
        name: "out".to_string(),
        args: vec![
            UGenExpr::Number(0.0),
            UGenExpr::UGenCall {
                name: "sample".to_string(),
                args: vec![UGenExpr::StringLit("kick.wav".to_string())],
                named_args: vec![("root".to_string(), UGenExpr::Number(60.0))],
            },
        ],
        named_args: na(),
    };
    let buffers = vec![BufferInfo {
        file_path: "kick.wav".to_string(),
        buffer_id: 0,
        num_channels: 2,
    }];
    let bytes = build_synthdef("test_sample", &["freq".to_string()], &body, &buffers).unwrap();
    let def = parse_synthdef(&bytes);
    let names = ugen_names(&def);
    assert!(names.contains(&"PlayBuf"));
    assert!(names.contains(&"BufRateScale"));
    let playbuf = def.ugens.iter().find(|u| u.name == "PlayBuf").unwrap();
    assert_eq!(playbuf.num_outputs, 2, "kick.wav is stereo");
    let bufnum_param = def.params.iter().find(|p| p.name == "bufnum").unwrap();
    assert_eq!(def.param_defaults[bufnum_param.index as usize], 0.0);
}

#[test]
fn test_sample_with_vel_range() {
    let body = UGenExpr::UGenCall {
        name: "out".to_string(),
        args: vec![
            UGenExpr::Number(0.0),
            UGenExpr::UGenCall {
                name: "sample".to_string(),
                args: vec![UGenExpr::StringLit("snare.wav".to_string())],
                named_args: vec![
                    ("root".to_string(), UGenExpr::Number(60.0)),
                    ("vel_lo".to_string(), UGenExpr::Number(0.0)),
                    ("vel_hi".to_string(), UGenExpr::Number(80.0)),
                ],
            },
        ],
        named_args: na(),
    };
    let buffers = vec![BufferInfo {
        file_path: "snare.wav".to_string(),
        buffer_id: 5,
        num_channels: 1,
    }];
    let bytes = build_synthdef("test_vel", &["freq".to_string()], &body, &buffers).unwrap();
    let def = parse_synthdef(&bytes);

    let vel_param = def.params.iter().find(|p| p.name == "vel").unwrap();
    assert_eq!(def.param_defaults[vel_param.index as usize], 127.0);
    let bufnum_param = def.params.iter().find(|p| p.name == "bufnum").unwrap();
    assert_eq!(def.param_defaults[bufnum_param.index as usize], 5.0);

    let names = ugen_names(&def);
    assert!(names.contains(&"PlayBuf"));
    let playbuf = def.ugens.iter().find(|u| u.name == "PlayBuf").unwrap();
    assert_eq!(playbuf.num_outputs, 1, "snare.wav is mono");
    assert!(def.constants.contains(&80.0), "vel_hi=80 should appear as a constant gate bound");
}

#[test]
fn test_collect_sample_paths() {
    let body = UGenExpr::BinOp {
        left: Box::new(UGenExpr::UGenCall {
            name: "sample".to_string(),
            args: vec![UGenExpr::StringLit("a.wav".to_string())],
            named_args: na(),
        }),
        op: BinOp::Add,
        right: Box::new(UGenExpr::UGenCall {
            name: "sample".to_string(),
            args: vec![UGenExpr::StringLit("b.wav".to_string())],
            named_args: na(),
        }),
    };
    let paths = collect_sample_paths(&body);
    assert_eq!(paths, vec!["a.wav", "b.wav"]);
}

#[test]
fn test_stream_disk() {
    let body = UGenExpr::UGenCall {
        name: "out".to_string(),
        args: vec![
            UGenExpr::Number(0.0),
            UGenExpr::UGenCall {
                name: "stream_disk".to_string(),
                args: vec![UGenExpr::Param("bufnum".to_string())],
                named_args: na(),
            },
        ],
        named_args: na(),
    };
    let bytes = build_synthdef("test_stream", &["bufnum".to_string()], &body, &[]).unwrap();
    let def = parse_synthdef(&bytes);
    let disk = def.ugens.iter().find(|u| u.name == "DiskIn").unwrap();
    assert_eq!(disk.num_outputs, 2, "default channels=2");
    assert_eq!(disk.num_inputs, 2, "DiskIn real inputs are [bufnum, loop]");
}

#[test]
fn test_stream_disk_mono_loop() {
    let body = UGenExpr::UGenCall {
        name: "out".to_string(),
        args: vec![
            UGenExpr::Number(0.0),
            UGenExpr::UGenCall {
                name: "stream_disk".to_string(),
                args: vec![UGenExpr::Param("bufnum".to_string())],
                named_args: vec![
                    ("channels".to_string(), UGenExpr::Number(1.0)),
                    ("loop".to_string(), UGenExpr::Number(1.0)),
                ],
            },
        ],
        named_args: na(),
    };
    let bytes = build_synthdef("test_mono", &["bufnum".to_string()], &body, &[]).unwrap();
    let def = parse_synthdef(&bytes);
    let disk = def.ugens.iter().find(|u| u.name == "DiskIn").unwrap();
    assert_eq!(disk.num_outputs, 1);
    assert!(def.constants.contains(&1.0));
}

#[test]
fn test_stream_disk_variable_rate() {
    let body = UGenExpr::UGenCall {
        name: "out".to_string(),
        args: vec![
            UGenExpr::Number(0.0),
            UGenExpr::UGenCall {
                name: "stream_disk_variable_rate".to_string(),
                args: vec![
                    UGenExpr::Param("bufnum".to_string()),
                    UGenExpr::Param("rate".to_string()),
                ],
                named_args: na(),
            },
        ],
        named_args: na(),
    };
    let bytes = build_synthdef(
        "test_vdisk",
        &["bufnum".to_string(), "rate".to_string()],
        &body,
        &[],
    )
    .unwrap();
    let def = parse_synthdef(&bytes);
    let disk = def.ugens.iter().find(|u| u.name == "VDiskIn").unwrap();
    assert_eq!(disk.num_outputs, 2);
    assert_eq!(disk.num_inputs, 4, "VDiskIn real inputs are [bufnum, rate, loop, sendID]");
}

#[test]
fn test_block_with_let() {
    let body = UGenExpr::Block {
        lets: vec![(
            "sig".to_string(),
            Box::new(UGenExpr::BinOp {
                left: Box::new(UGenExpr::BinOp {
                    left: Box::new(UGenExpr::UGenCall {
                        name: "saw".to_string(),
                        args: vec![UGenExpr::Param("freq".to_string())],
                        named_args: na(),
                    }),
                    op: BinOp::Mul,
                    right: Box::new(UGenExpr::UGenCall {
                        name: "env".to_string(),
                        args: vec![UGenExpr::Param("gate".to_string())],
                        named_args: na(),
                    }),
                }),
                op: BinOp::Mul,
                right: Box::new(UGenExpr::Param("amp".to_string())),
            }),
        )],
        results: vec![Box::new(UGenExpr::UGenCall {
            name: "out".to_string(),
            args: vec![
                UGenExpr::Number(0.0),
                UGenExpr::UGenCall {
                    name: "reverb".to_string(),
                    args: vec![
                        UGenExpr::Param("sig".to_string()),
                        UGenExpr::Number(0.5),
                        UGenExpr::Number(0.8),
                        UGenExpr::Number(0.5),
                    ],
                    named_args: na(),
                },
            ],
            named_args: na(),
        })],
    };
    let bytes = build_synthdef(
        "test_verb",
        &["freq".to_string(), "amp".to_string(), "gate".to_string()],
        &body,
        &[],
    )
    .unwrap();
    let def = parse_synthdef(&bytes);
    let names = ugen_names(&def);
    assert!(names.contains(&"FreeVerb"), "let-bound sig should still flow into reverb: {:?}", names);
    let verb = def.ugens.iter().find(|u| u.name == "FreeVerb").unwrap();
    assert_eq!(verb.num_inputs, 4);
}

#[test]
fn test_block_multiple_lets() {
    let body = UGenExpr::Block {
        lets: vec![
            (
                "dry".to_string(),
                Box::new(UGenExpr::UGenCall {
                    name: "saw".to_string(),
                    args: vec![UGenExpr::Param("freq".to_string())],
                    named_args: na(),
                }),
            ),
            (
                "wet".to_string(),
                Box::new(UGenExpr::UGenCall {
                    name: "delay".to_string(),
                    args: vec![
                        UGenExpr::Param("dry".to_string()),
                        UGenExpr::Number(0.2),
                        UGenExpr::Number(2.0),
                    ],
                    named_args: na(),
                }),
            ),
        ],
        results: vec![Box::new(UGenExpr::UGenCall {
            name: "out".to_string(),
            args: vec![
                UGenExpr::Number(0.0),
                UGenExpr::BinOp {
                    left: Box::new(UGenExpr::Param("dry".to_string())),
                    op: BinOp::Add,
                    right: Box::new(UGenExpr::Param("wet".to_string())),
                },
            ],
            named_args: na(),
        })],
    };
    let bytes = build_synthdef("test_multi", &["freq".to_string()], &body, &[]).unwrap();
    let def = parse_synthdef(&bytes);
    let names = ugen_names(&def);
    assert!(names.contains(&"Saw"));
    assert!(names.contains(&"CombL"), "delay() should lower to CombL: {:?}", names);
    assert!(names.contains(&"BinaryOpUGen"), "dry + wet should be a BinaryOpUGen add");
}

#[test]
fn test_lfo_sine() {
    let body = UGenExpr::UGenCall {
        name: "out".to_string(),
        args: vec![
            UGenExpr::Number(0.0),
            UGenExpr::UGenCall {
                name: "lfo_sine".to_string(),
                args: vec![UGenExpr::Number(0.5)],
                named_args: na(),
            },
        ],
        named_args: na(),
    };
    let bytes = build_synthdef("test_lfo", &[], &body, &[]).unwrap();
    let def = parse_synthdef(&bytes);
    let lfo = def.ugens.iter().find(|u| u.name == "SinOsc").unwrap();
    assert_eq!(lfo.rate, 1, "lfo_sine should be control-rate");
}

#[test]
fn test_lfo_all_types() {
    let lfo_tests = vec![
        ("lfo_sine", "SinOsc"),
        ("lfo_saw", "LFSaw"),
        ("lfo_tri", "LFTri"),
        ("lfo_noise", "LFNoise1"),
        ("lfo_step", "LFNoise0"),
    ];
    for (audion_name, expected_ugen) in lfo_tests {
        let body = UGenExpr::UGenCall {
            name: audion_name.to_string(),
            args: vec![UGenExpr::Number(1.0)],
            named_args: na(),
        };
        let bytes = build_synthdef("test", &[], &body, &[]).unwrap();
        let def = parse_synthdef(&bytes);
        let names = ugen_names(&def);
        assert!(
            names.contains(&expected_ugen),
            "{} should emit {}: {:?}",
            audion_name,
            expected_ugen,
            names
        );
    }
}

#[test]
fn test_lfo_pulse_with_width() {
    let body = UGenExpr::UGenCall {
        name: "lfo_pulse".to_string(),
        args: vec![UGenExpr::Number(2.0), UGenExpr::Number(0.3)],
        named_args: na(),
    };
    let bytes = build_synthdef("test", &[], &body, &[]).unwrap();
    let def = parse_synthdef(&bytes);
    assert!(def.constants.contains(&0.3));
    let pulse = def.ugens.iter().find(|u| u.name == "LFPulse").unwrap();
    assert_eq!(pulse.rate, 1);
    assert_eq!(pulse.num_inputs, 3, "LFPulse(freq, iphase, width)");
}

#[test]
fn test_collect_sample_paths_in_block() {
    let body = UGenExpr::Block {
        lets: vec![(
            "sig".to_string(),
            Box::new(UGenExpr::UGenCall {
                name: "sample".to_string(),
                args: vec![UGenExpr::StringLit("kick.wav".to_string())],
                named_args: na(),
            }),
        )],
        results: vec![Box::new(UGenExpr::UGenCall {
            name: "out".to_string(),
            args: vec![UGenExpr::Number(0.0), UGenExpr::Param("sig".to_string())],
            named_args: na(),
        })],
    };
    let paths = collect_sample_paths(&body);
    assert_eq!(paths, vec!["kick.wav"]);
}
