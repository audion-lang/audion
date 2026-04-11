mod common;
use common::eval;
use audion::value::Value;

#[test]
fn test_array_mel_debruijn_k() {
    // Test ternary (k=3) de Bruijn of order 2
    let result = eval("array_mel_debruijn_k(3, 2, 0);");
    match result {
        Value::String(_) => assert!(true), // Returns a string
        _ => panic!("Expected string result"),
    }
}

#[test]
fn test_array_mel_lattice_walk_square() {
    // Simple 2x2 grid, adjacent corners, 2 steps
    let result = eval("array_mel_lattice_walk_square(2, 2, 0, 0, 1, 1, 2);");
    match result {
        Value::Array(arr) => {
            let locked = arr.lock().unwrap();
            let entries = locked.entries();
            assert!(entries.len() > 0); // Should find at least one path
        }
        _ => panic!("Expected array result"),
    }
}

#[test]
fn test_array_mel_lattice_walk_tri() {
    // Triangular lattice includes diagonals
    let result = eval("array_mel_lattice_walk_tri(2, 2, 0, 0, 1, 1, 1);");
    match result {
        Value::Array(arr) => {
            let locked = arr.lock().unwrap();
            let entries = locked.entries();
            assert!(entries.len() > 0); // Should find paths
        }
        _ => panic!("Expected array result"),
    }
}

#[test]
fn test_array_mel_string_to_indices() {
    // Map "012" with 3 notes → [0, 1, 2]
    let result = eval(r#"array_mel_string_to_indices("012", 3);"#);
    if let Value::Array(arr) = result {
        let locked = arr.lock().unwrap();
        let entries = locked.entries();
        assert_eq!(entries.len(), 3);
        assert_eq!(entries[0].1, Value::Number(0.0));
        assert_eq!(entries[1].1, Value::Number(1.0));
        assert_eq!(entries[2].1, Value::Number(2.0));
    } else {
        panic!("Expected array result");
    }

    // Map "abc" with 7 notes → indices mod 7
    let result = eval(r#"array_mel_string_to_indices("abc", 7);"#);
    if let Value::Array(arr) = result {
        let locked = arr.lock().unwrap();
        let entries = locked.entries();
        assert_eq!(entries.len(), 3);
        assert_eq!(entries[0].1, Value::Number(3.0)); // 'a' = 10, 10 % 7 = 3
        assert_eq!(entries[1].1, Value::Number(4.0)); // 'b' = 11, 11 % 7 = 4
        assert_eq!(entries[2].1, Value::Number(5.0)); // 'c' = 12, 12 % 7 = 5
    } else {
        panic!("Expected array result");
    }
}

#[test]
fn test_array_mel_random_walk() {
    // Random walk: start=5, min=0, max=10, step=1, length=5
    let result = eval("array_mel_random_walk(5, 0, 10, 1, 5);");
    if let Value::Array(arr) = result {
        let locked = arr.lock().unwrap();
        let entries = locked.entries();
        assert_eq!(entries.len(), 5);
        // Check all values are within bounds
        for (_k, v) in entries {
            if let Value::Number(n) = v {
                assert!(*n >= 0.0 && *n <= 10.0);
            } else {
                panic!("Expected numbers in array");
            }
        }
    } else {
        panic!("Expected array result");
    }
}

#[test]
fn test_array_mel_invert() {
    // Invert [60, 62, 64] around pivot 62
    let result = eval("array_mel_invert([60, 62, 64], 62);");
    if let Value::Array(arr) = result {
        let locked = arr.lock().unwrap();
        let entries = locked.entries();
        assert_eq!(entries.len(), 3);
        assert_eq!(entries[0].1, Value::Number(64.0));
        assert_eq!(entries[1].1, Value::Number(62.0));
        assert_eq!(entries[2].1, Value::Number(60.0));
    } else {
        panic!("Expected array result");
    }
}

#[test]
fn test_array_mel_reverse() {
    // Reverse [1, 2, 3] → [3, 2, 1]
    let result = eval("array_mel_reverse([1, 2, 3]);");
    if let Value::Array(arr) = result {
        let locked = arr.lock().unwrap();
        let entries = locked.entries();
        assert_eq!(entries.len(), 3);
        assert_eq!(entries[0].1, Value::Number(3.0));
        assert_eq!(entries[1].1, Value::Number(2.0));
        assert_eq!(entries[2].1, Value::Number(1.0));
    } else {
        panic!("Expected array result");
    }
}

#[test]
fn test_array_mel_transformations_combined() {
    // Test retrograde inversion
    let result = eval("let orig = [1, 2, 3]; let inv = array_mel_invert(orig, 2); array_mel_reverse(inv);");
    if let Value::Array(arr) = result {
        let locked = arr.lock().unwrap();
        let entries = locked.entries();
        assert_eq!(entries.len(), 3);
        // [1,2,3] inverted around 2 is [3,2,1], reversed is [1,2,3]
        assert_eq!(entries[0].1, Value::Number(1.0));
        assert_eq!(entries[1].1, Value::Number(2.0));
        assert_eq!(entries[2].1, Value::Number(3.0));
    } else {
        panic!("Expected array result");
    }
}

//
// Tests for newly ported functions
//

#[test]
fn test_array_mel_lattice_walk_square_no_retrace() {
    // 2x2 grid, from (0,0) to (1,1), 2 steps
    // Cannot retrace previous step
    let result = eval("array_mel_lattice_walk_square_no_retrace(2, 2, 0, 0, 1, 1, 2);");
    match result {
        Value::Array(arr) => {
            let locked = arr.lock().unwrap();
            let entries = locked.entries();
            assert!(entries.len() > 0, "Should find at least one path");

            // Check that paths don't retrace
            for (_k, v) in entries {
                if let Value::String(path) = v {
                    // Check no immediate reversals: rl, lr, ud, du
                    assert!(!path.contains("rl"), "Path should not contain 'rl' (retrace)");
                    assert!(!path.contains("lr"), "Path should not contain 'lr' (retrace)");
                    assert!(!path.contains("ud"), "Path should not contain 'ud' (retrace)");
                    assert!(!path.contains("du"), "Path should not contain 'du' (retrace)");
                } else {
                    panic!("Expected string path");
                }
            }
        }
        _ => panic!("Expected array result"),
    }
}

#[test]
fn test_array_mel_lattice_walk_square_with_stops() {
    // 2x2 grid, from (0,0) to (0,0), max 2 stops, 3 steps
    // Can stay in place
    let result = eval("array_mel_lattice_walk_square_with_stops(2, 2, 0, 0, 0, 0, 2, 3);");
    match result {
        Value::Array(arr) => {
            let locked = arr.lock().unwrap();
            let entries = locked.entries();
            assert!(entries.len() > 0, "Should find at least one path");

            // Check that some paths contain 's' (stop)
            let has_stops = entries.iter().any(|(_k, v)| {
                if let Value::String(path) = v {
                    path.contains('s')
                } else {
                    false
                }
            });
            assert!(has_stops, "At least some paths should contain stop moves");
        }
        _ => panic!("Expected array result"),
    }
}

#[test]
fn test_array_mel_subset_sample() {
    // Sample 3 elements from [1,2,3,4,5]
    let result = eval("array_mel_subset_sample([1,2,3,4,5], 3);");
    match result {
        Value::Array(arr) => {
            let locked = arr.lock().unwrap();
            let entries = locked.entries();
            assert_eq!(entries.len(), 3, "Should return exactly 3 elements");

            // All values should be from the original array
            for (_k, v) in entries {
                if let Value::Number(n) = v {
                    assert!(*n >= 1.0 && *n <= 5.0, "Sampled value should be in [1,5]");
                } else {
                    panic!("Expected number in sampled array");
                }
            }
        }
        _ => panic!("Expected array result"),
    }
}

#[test]
fn test_array_mel_subset_sample_edge_cases() {
    // Sample 0 elements
    let result = eval("array_mel_subset_sample([1,2,3], 0);");
    match result {
        Value::Array(arr) => {
            let locked = arr.lock().unwrap();
            let entries = locked.entries();
            assert_eq!(entries.len(), 0, "Should return empty array");
        }
        _ => panic!("Expected array result"),
    }

    // Sample all elements
    let result = eval("array_mel_subset_sample([1,2,3], 3);");
    match result {
        Value::Array(arr) => {
            let locked = arr.lock().unwrap();
            let entries = locked.entries();
            assert_eq!(entries.len(), 3, "Should return all 3 elements");
        }
        _ => panic!("Expected array result"),
    }
}

#[test]
fn test_array_mel_lattice_to_melody() {
    // Create a 2x2 note grid and follow a simple path
    let result = eval(r#"
        let grid = [[60, 62], [64, 65]];
        array_mel_lattice_to_melody(grid, "rru", 0, 0);
    "#);
    match result {
        Value::Array(arr) => {
            let locked = arr.lock().unwrap();
            let entries = locked.entries();
            assert_eq!(entries.len(), 4, "Should have 4 notes (start + 3 moves)");
            // Start at (0,0) = 60, right to (1,0) = 62, right wraps to (0,0) = 60, up to (0,1) = 64
            assert_eq!(entries[0].1, Value::Number(60.0));
            assert_eq!(entries[1].1, Value::Number(62.0));
            assert_eq!(entries[2].1, Value::Number(60.0)); // wrapped
            assert_eq!(entries[3].1, Value::Number(64.0));
        }
        _ => panic!("Expected array result"),
    }
}

#[test]
fn test_array_mel_lattice_to_melody_with_stops() {
    // Test the 's' (stop) command - should repeat current note
    let result = eval(r#"
        let grid = [[1, 2], [3, 4]];
        array_mel_lattice_to_melody(grid, "ss", 0, 0);
    "#);
    match result {
        Value::Array(arr) => {
            let locked = arr.lock().unwrap();
            let entries = locked.entries();
            assert_eq!(entries.len(), 3, "Should have 3 notes (start + 2 stops)");
            // All should be the same note (staying at 0,0)
            assert_eq!(entries[0].1, Value::Number(1.0));
            assert_eq!(entries[1].1, Value::Number(1.0));
            assert_eq!(entries[2].1, Value::Number(1.0));
        }
        _ => panic!("Expected array result"),
    }
}

#[test]
fn test_array_mel_automaton() {
    // Simple automaton: state 0 -> [(1, "a")], state 1 -> [(0, "b")]
    // Start at 0, end at 0, length 2
    // Should generate: "ab"
    let result = eval(r#"
        let aut = [
            [[1, "a"]],
            [[0, "b"]]
        ];
        array_mel_automaton(aut, 0, [0], 2);
    "#);
    match result {
        Value::Array(arr) => {
            let locked = arr.lock().unwrap();
            let entries = locked.entries();
            assert!(entries.len() > 0, "Should generate at least one word");

            // Check that we got "ab"
            let has_ab = entries.iter().any(|(_k, v)| {
                if let Value::String(s) = v {
                    s == "ab"
                } else {
                    false
                }
            });
            assert!(has_ab, "Should generate word 'ab'");
        }
        _ => panic!("Expected array result"),
    }
}

#[test]
fn test_array_mel_automaton_multiple_paths() {
    // Automaton with branching
    let result = eval(r#"
        let aut = [
            [[1, "a"], [2, "b"]],
            [[0, "c"]],
            [[0, "d"]]
        ];
        array_mel_automaton(aut, 0, [0], 2);
    "#);
    match result {
        Value::Array(arr) => {
            let locked = arr.lock().unwrap();
            let entries = locked.entries();
            assert_eq!(entries.len(), 2, "Should generate 2 words");

            // Should have "ac" and "bd"
            let words: Vec<String> = entries.iter()
                .filter_map(|(_k, v)| {
                    if let Value::String(s) = v {
                        Some(s.clone())
                    } else {
                        None
                    }
                })
                .collect();
            assert!(words.contains(&"ac".to_string()));
            assert!(words.contains(&"bd".to_string()));
        }
        _ => panic!("Expected array result"),
    }
}

#[test]
fn test_array_mel_probabilistic_automaton() {
    // Simple probabilistic automaton
    // State 0 has transitions: (1, "a", 0.5), (2, "b", 0.5)
    // Both lead to terminal states
    let result = eval(r#"
        let aut = [
            [[1, "a", 0.5], [2, "b", 0.5]],
            [[1, "x", 1.0]],
            [[2, "y", 1.0]]
        ];
        array_mel_probabilistic_automaton(aut, 0, 2);
    "#);
    match result {
        Value::String(s) => {
            assert_eq!(s.len(), 2, "Should generate a 2-character word");
            // Should be either "ax" or "by"
            assert!(s == "ax" || s == "by", "Generated word should be 'ax' or 'by', got '{}'", s);
        }
        _ => panic!("Expected string result"),
    }
}

#[test]
fn test_array_mel_probabilistic_automaton_deterministic() {
    // Fully deterministic (probability = 1.0)
    let result = eval(r#"
        let aut = [
            [[1, "hello", 1.0]],
            [[2, "world", 1.0]],
            [[2, "!", 1.0]]
        ];
        array_mel_probabilistic_automaton(aut, 0, 2);
    "#);
    match result {
        Value::String(s) => {
            assert_eq!(s, "helloworld", "Should generate 'helloworld'");
        }
        _ => panic!("Expected string result"),
    }
}
