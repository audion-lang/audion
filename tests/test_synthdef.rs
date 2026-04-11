use audion::ast::{BinOp, UGenExpr};
use audion::synthdef::{generate_sclang, collect_sample_paths, BufferInfo};

fn na() -> Vec<(String, UGenExpr)> {
    vec![]
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
    let code = generate_sclang("test_sine", &["freq".to_string()], &body, "/tmp", &[]);
    assert!(code.contains("SynthDef(\\test_sine"));
    assert!(code.contains("SinOsc.ar(freq)"));
    assert!(code.contains("Out.ar(0, SinOsc.ar(freq))"));
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
    let code = generate_sclang("test_saw", &["freq".to_string()], &body, "/tmp", &[]);
    assert!(code.contains("LPF.ar(Saw.ar(freq), 2000)"));
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
    let code = generate_sclang(
        "test_env",
        &["freq".to_string(), "gate".to_string()],
        &body,
        "/tmp",
        &[],
    );
    assert!(code.contains("EnvGen.kr(Env.asr(0.01, 1, 0.3), gate, doneAction: 2)"));
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
    let code = generate_sclang(
        "test_defaults",
        &["freq".to_string(), "amp".to_string(), "out".to_string()],
        &body,
        "/tmp",
        &[],
    );
    assert!(code.contains("freq=440"));
    assert!(code.contains("amp=0.1"));
    assert!(code.contains("out=0"));
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
                named_args: vec![
                    ("root".to_string(), UGenExpr::Number(60.0)),
                ],
            },
        ],
        named_args: na(),
    };
    let buffers = vec![BufferInfo {
        file_path: "kick.wav".to_string(),
        buffer_id: 0,
        num_channels: 2,
    }];
    let code = generate_sclang("test_sample", &["freq".to_string()], &body, "/tmp", &buffers);
    assert!(code.contains("PlayBuf.ar(2, bufnum"));
    assert!(code.contains("BufRateScale.kr(bufnum)"));
    assert!(code.contains("bufnum=0"));
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
    let code = generate_sclang("test_vel", &["freq".to_string()], &body, "/tmp", &buffers);
    assert!(code.contains("vel=127"));
    assert!(code.contains("(vel >= 0)"));
    assert!(code.contains("(vel <= 80)"));
    assert!(code.contains("PlayBuf.ar(1, bufnum"));
    assert!(code.contains("bufnum=5"));
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
    let code = generate_sclang("test_stream", &["bufnum".to_string()], &body, "/tmp", &[]);
    assert!(code.contains("DiskIn.ar(2, bufnum, 0)"));
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
    let code = generate_sclang("test_mono", &["bufnum".to_string()], &body, "/tmp", &[]);
    assert!(code.contains("DiskIn.ar(1, bufnum, 1)"));
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
    let code = generate_sclang(
        "test_vdisk",
        &["bufnum".to_string(), "rate".to_string()],
        &body,
        "/tmp",
        &[],
    );
    assert!(code.contains("VDiskIn.ar(2, bufnum, rate, 0)"));
}

#[test]
fn test_block_with_let() {
    let body = UGenExpr::Block {
        lets: vec![
            ("sig".to_string(), Box::new(UGenExpr::BinOp {
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
            })),
        ],
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
    let code = generate_sclang(
        "test_verb",
        &["freq".to_string(), "amp".to_string(), "gate".to_string()],
        &body,
        "/tmp",
        &[],
    );
    assert!(code.contains("var sig;"), "should declare var: {}", code);
    assert!(code.contains("sig = ((Saw.ar(freq) * EnvGen.kr"), "should assign sig: {}", code);
    assert!(code.contains("FreeVerb.ar(sig, 0.5, 0.8, 0.5)"), "should use sig in reverb: {}", code);
}

#[test]
fn test_block_multiple_lets() {
    let body = UGenExpr::Block {
        lets: vec![
            ("dry".to_string(), Box::new(UGenExpr::UGenCall {
                name: "saw".to_string(),
                args: vec![UGenExpr::Param("freq".to_string())],
                named_args: na(),
            })),
            ("wet".to_string(), Box::new(UGenExpr::UGenCall {
                name: "delay".to_string(),
                args: vec![
                    UGenExpr::Param("dry".to_string()),
                    UGenExpr::Number(0.2),
                    UGenExpr::Number(2.0),
                ],
                named_args: na(),
            })),
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
    let code = generate_sclang(
        "test_multi",
        &["freq".to_string()],
        &body,
        "/tmp",
        &[],
    );
    assert!(code.contains("var dry, wet;"), "should declare both vars: {}", code);
    assert!(code.contains("dry = Saw.ar(freq);"), "should assign dry: {}", code);
    assert!(code.contains("wet = CombL.ar(dry, 0.2, 0.2, 2);"), "should assign wet: {}", code);
    assert!(code.contains("Out.ar(0, (dry + wet))"), "should use both in output: {}", code);
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
    let code = generate_sclang("test_lfo", &[], &body, "/tmp", &[]);
    assert!(code.contains("SinOsc.kr(0.5)"), "lfo_sine should emit SinOsc.kr: {}", code);
}

#[test]
fn test_lfo_all_types() {
    let lfo_tests = vec![
        ("lfo_sine", "SinOsc.kr(1)"),
        ("lfo_saw", "LFSaw.kr(1)"),
        ("lfo_tri", "LFTri.kr(1)"),
        ("lfo_noise", "LFNoise1.kr(1)"),
        ("lfo_step", "LFNoise0.kr(1)"),
    ];
    for (audion_name, expected_sc) in lfo_tests {
        let body = UGenExpr::UGenCall {
            name: audion_name.to_string(),
            args: vec![UGenExpr::Number(1.0)],
            named_args: na(),
        };
        let code = generate_sclang("test", &[], &body, "/tmp", &[]);
        assert!(code.contains(expected_sc), "{} should emit {}: {}", audion_name, expected_sc, code);
    }
}

#[test]
fn test_lfo_pulse_with_width() {
    let body = UGenExpr::UGenCall {
        name: "lfo_pulse".to_string(),
        args: vec![UGenExpr::Number(2.0), UGenExpr::Number(0.3)],
        named_args: na(),
    };
    let code = generate_sclang("test", &[], &body, "/tmp", &[]);
    assert!(code.contains("LFPulse.kr(2, 0, 0.3)"), "lfo_pulse should emit LFPulse.kr with width: {}", code);
}

#[test]
fn test_collect_sample_paths_in_block() {
    let body = UGenExpr::Block {
        lets: vec![
            ("sig".to_string(), Box::new(UGenExpr::UGenCall {
                name: "sample".to_string(),
                args: vec![UGenExpr::StringLit("kick.wav".to_string())],
                named_args: na(),
            })),
        ],
        results: vec![Box::new(UGenExpr::UGenCall {
            name: "out".to_string(),
            args: vec![UGenExpr::Number(0.0), UGenExpr::Param("sig".to_string())],
            named_args: na(),
        })],
    };
    let paths = collect_sample_paths(&body);
    assert_eq!(paths, vec!["kick.wav"]);
}
