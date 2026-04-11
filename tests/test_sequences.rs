mod common;
use common::eval;
use audion::value::Value;

// ---------------------------------------------------------------------------
// Binary/Interval Conversion Tests
// ---------------------------------------------------------------------------

#[test]
fn test_binary_to_intervals_basic() {
    let result = eval(r#"array_seq_binary_to_intervals("1010010001001000");"#);

    // Should return [2, 3, 4, 3, 4]
    if let Value::Array(arr) = result {
        let locked = arr.lock().unwrap();
        let entries = locked.entries();
        assert_eq!(entries.len(), 5);
        assert_eq!(entries[0].1, Value::Number(2.0));
        assert_eq!(entries[1].1, Value::Number(3.0));
        assert_eq!(entries[2].1, Value::Number(4.0));
        assert_eq!(entries[3].1, Value::Number(3.0));
        assert_eq!(entries[4].1, Value::Number(4.0));
    } else {
        panic!("Expected array result");
    }
}

#[test]
fn test_binary_to_intervals_single_one() {
    let result = eval(r#"array_seq_binary_to_intervals("1");"#);

    if let Value::Array(arr) = result {
        let locked = arr.lock().unwrap();
        let entries = locked.entries();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].1, Value::Number(1.0));
    } else {
        panic!("Expected array result");
    }
}

#[test]
fn test_binary_to_intervals_trailing_zeros() {
    let result = eval(r#"array_seq_binary_to_intervals("1000");"#);

    if let Value::Array(arr) = result {
        let locked = arr.lock().unwrap();
        let entries = locked.entries();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].1, Value::Number(4.0));
    } else {
        panic!("Expected array result");
    }
}

#[test]
fn test_binary_to_intervals_consecutive_ones() {
    let result = eval(r#"array_seq_binary_to_intervals("111");"#);

    if let Value::Array(arr) = result {
        let locked = arr.lock().unwrap();
        let entries = locked.entries();
        assert_eq!(entries.len(), 3);
        assert_eq!(entries[0].1, Value::Number(1.0));
        assert_eq!(entries[1].1, Value::Number(1.0));
        assert_eq!(entries[2].1, Value::Number(1.0));
    } else {
        panic!("Expected array result");
    }
}

#[test]
fn test_binary_to_intervals_empty() {
    let result = eval(r#"array_seq_binary_to_intervals("");"#);

    if let Value::Array(arr) = result {
        let locked = arr.lock().unwrap();
        assert_eq!(locked.len(), 0);
    } else {
        panic!("Expected array result");
    }
}

#[test]
fn test_intervals_to_binary_basic() {
    let result = eval(r#"array_seq_intervals_to_binary([2, 3, 4, 3, 4]);"#);
    assert_eq!(result, Value::String("1010010001001000".to_string()));
}

#[test]
fn test_intervals_to_binary_single() {
    let result = eval(r#"array_seq_intervals_to_binary([1]);"#);
    assert_eq!(result, Value::String("1".to_string()));
}

#[test]
fn test_intervals_to_binary_all_ones() {
    let result = eval(r#"array_seq_intervals_to_binary([1, 1, 1]);"#);
    assert_eq!(result, Value::String("111".to_string()));
}

#[test]
fn test_intervals_to_binary_longer() {
    let result = eval(r#"array_seq_intervals_to_binary([5]);"#);
    assert_eq!(result, Value::String("10000".to_string()));
}

#[test]
fn test_intervals_to_binary_empty() {
    let result = eval(r#"array_seq_intervals_to_binary([]);"#);
    assert_eq!(result, Value::String("".to_string()));
}

#[test]
fn test_binary_intervals_roundtrip() {
    // Test that converting to intervals and back gives the same result
    let result = eval(r#"
        let binary = "10101001";
        let intervals = array_seq_binary_to_intervals(binary);
        let back = array_seq_intervals_to_binary(intervals);
        back;
    "#);
    assert_eq!(result, Value::String("10101001".to_string()));
}

// ---------------------------------------------------------------------------
// Random Correlated Tests
// ---------------------------------------------------------------------------

#[test]
fn test_random_correlated_zero_correlation() {
    // With c=0, all values should be the same as s
    let result = eval(r#"array_seq_random_correlated(10, 5, 0, 8);"#);

    if let Value::Array(arr) = result {
        let locked = arr.lock().unwrap();
        let entries = locked.entries();
        assert_eq!(entries.len(), 8);
        for (_key, val) in entries {
            assert_eq!(*val, Value::Number(5.0));
        }
    } else {
        panic!("Expected array result");
    }
}

#[test]
fn test_random_correlated_length() {
    let result = eval(r#"array_seq_random_correlated(10, 5, 5, 20);"#);

    if let Value::Array(arr) = result {
        let locked = arr.lock().unwrap();
        assert_eq!(locked.len(), 20);
    } else {
        panic!("Expected array result");
    }
}

#[test]
fn test_random_correlated_range() {
    // All values should be in range [0, m]
    let result = eval(r#"
        seed(12345);
        array_seq_random_correlated(10, 5, 8, 50);
    "#);

    if let Value::Array(arr) = result {
        let locked = arr.lock().unwrap();
        let entries = locked.entries();
        for (_key, val) in entries {
            if let Value::Number(n) = val {
                assert!(*n >= 0.0 && *n <= 10.0, "Value {} out of range [0, 10]", n);
            } else {
                panic!("Expected number in array");
            }
        }
    } else {
        panic!("Expected array result");
    }
}

#[test]
fn test_random_correlated_seeded_reproducible() {
    // With same seed, should get same results
    let result1 = eval(r#"
        seed(42);
        array_seq_random_correlated(10, 5, 5, 10);
    "#);

    let result2 = eval(r#"
        seed(42);
        array_seq_random_correlated(10, 5, 5, 10);
    "#);

    assert_eq!(result1, result2);
}

#[test]
fn test_random_correlated_different_seeds() {
    // With different seeds, should get different results (highly probable)
    let result1 = eval(r#"
        seed(42);
        array_seq_random_correlated(10, 5, 8, 20);
    "#);

    let result2 = eval(r#"
        seed(999);
        array_seq_random_correlated(10, 5, 8, 20);
    "#);

    assert_ne!(result1, result2);
}

#[test]
fn test_random_correlated_edge_case_single() {
    let result = eval(r#"array_seq_random_correlated(10, 5, 0, 1);"#);

    if let Value::Array(arr) = result {
        let locked = arr.lock().unwrap();
        assert_eq!(locked.len(), 1);
    } else {
        panic!("Expected array result");
    }
}

#[test]
fn test_random_correlated_max_correlation() {
    // With c=m, correlation is maximized (independent random values)
    let result = eval(r#"
        seed(42);
        array_seq_random_correlated(10, 5, 10, 30);
    "#);

    if let Value::Array(arr) = result {
        let locked = arr.lock().unwrap();
        let entries = locked.entries();

        // Should have some variation (not all the same)
        let first_val = &entries[0].1;
        let has_variation = entries.iter().any(|(_k, v)| v != first_val);
        assert!(has_variation, "Expected variation with max correlation");
    } else {
        panic!("Expected array result");
    }
}

// ---------------------------------------------------------------------------
// Integer Partition Tests
// ---------------------------------------------------------------------------

#[test]
fn test_partitions_basic() {
    // Partitions of 4: [1,1,1,1], [1,1,2], [1,3], [2,2], [4]
    let result = eval(r#"array_seq_partitions(4);"#);

    if let Value::Array(arr) = result {
        let locked = arr.lock().unwrap();
        assert_eq!(locked.len(), 5);
    } else {
        panic!("Expected array result");
    }
}

#[test]
fn test_partitions_small() {
    // Partitions of 1: [1]
    let result = eval(r#"array_seq_partitions(1);"#);

    if let Value::Array(arr) = result {
        let locked = arr.lock().unwrap();
        assert_eq!(locked.len(), 1);
    } else {
        panic!("Expected array result");
    }
}

#[test]
fn test_partitions_allowed() {
    // Partitions of 8 using only 2s and 3s: [2,2,2,2], [2,3,3], [3,3,2] (order-independent so [2,3,3] only)
    let result = eval(r#"array_seq_partitions_allowed(8, [2, 3]);"#);

    if let Value::Array(arr) = result {
        let locked = arr.lock().unwrap();
        // Should find partitions like [2,2,2,2] and [2,3,3]
        assert!(locked.len() > 0);
    } else {
        panic!("Expected array result");
    }
}

#[test]
fn test_partitions_m_parts() {
    // Partitions of 5 into exactly 2 parts: [1,4], [2,3]
    let result = eval(r#"array_seq_partitions_m_parts(5, 2);"#);

    if let Value::Array(arr) = result {
        let locked = arr.lock().unwrap();
        assert_eq!(locked.len(), 2);
    } else {
        panic!("Expected array result");
    }
}

#[test]
fn test_partitions_allowed_m_parts() {
    // Partitions of 6 into 2 parts using only [2, 3]: [3,3]
    let result = eval(r#"array_seq_partitions_allowed_m_parts(6, 2, [2, 3]);"#);

    if let Value::Array(arr) = result {
        let locked = arr.lock().unwrap();
        assert_eq!(locked.len(), 1);

        // Check it's [3,3]
        let entries = locked.entries();
        if let Value::Array(part_arr) = &entries[0].1 {
            let part_locked = part_arr.lock().unwrap();
            let part_entries = part_locked.entries();
            assert_eq!(part_entries.len(), 2);
            assert_eq!(part_entries[0].1, Value::Number(3.0));
            assert_eq!(part_entries[1].1, Value::Number(3.0));
        }
    } else {
        panic!("Expected array result");
    }
}

// ---------------------------------------------------------------------------
// Necklace Tests
// ---------------------------------------------------------------------------

#[test]
fn test_necklaces_basic() {
    // Binary necklaces of length 4
    let result = eval(r#"array_seq_necklaces(4);"#);

    if let Value::Array(arr) = result {
        let locked = arr.lock().unwrap();
        // Should have several necklaces
        assert!(locked.len() > 0);

        // All results should be strings
        for (_key, val) in locked.entries() {
            assert!(matches!(val, Value::String(_)));
        }
    } else {
        panic!("Expected array result");
    }
}

#[test]
fn test_necklaces_small() {
    // Binary necklaces of length 3
    let result = eval(r#"array_seq_necklaces(3);"#);

    if let Value::Array(arr) = result {
        let locked = arr.lock().unwrap();
        // For length 3: "000", "001", "011", "111"
        assert!(locked.len() >= 3);
    } else {
        panic!("Expected array result");
    }
}

#[test]
fn test_necklaces_allowed() {
    // Necklaces with allowed run lengths
    let result = eval(r#"array_seq_necklaces_allowed(8, [2, 3]);"#);

    if let Value::Array(arr) = result {
        let locked = arr.lock().unwrap();
        assert!(locked.len() > 0);
    } else {
        panic!("Expected array result");
    }
}

#[test]
fn test_necklaces_m_ones() {
    // Binary necklaces of length 6 with exactly 2 ones
    let result = eval(r#"array_seq_necklaces_m_ones(6, 2);"#);

    if let Value::Array(arr) = result {
        let locked = arr.lock().unwrap();
        assert!(locked.len() > 0);

        // Verify at least one has exactly 2 ones
        let entries = locked.entries();
        if let Value::String(s) = &entries[0].1 {
            let ones_count = s.chars().filter(|&c| c == '1').count();
            assert_eq!(ones_count, 2);
        }
    } else {
        panic!("Expected array result");
    }
}

#[test]
fn test_necklaces_allowed_m_ones() {
    // Necklaces of length 8 with 3 ones and allowed parts
    let result = eval(r#"array_seq_necklaces_allowed_m_ones(8, 3, [2, 3]);"#);

    if let Value::Array(_arr) = result {
        // Should find some necklaces (may be 0 or more depending on constraints)
        // Just verify it returns an array - length can be 0
    } else {
        panic!("Expected array result");
    }
}

// ---------------------------------------------------------------------------
// Markov Chain Tests
// ---------------------------------------------------------------------------

#[test]
fn test_markov_basic() {
    // Simple 2-state Markov chain
    let result = eval(r#"
        seed(42);
        let matrix = [
            [0.8, 0.2],
            [0.3, 0.7]
        ];
        array_seq_markov(matrix, 0, 10);
    "#);

    if let Value::Array(arr) = result {
        let locked = arr.lock().unwrap();
        assert_eq!(locked.len(), 10);

        // All values should be 0 or 1 (states)
        for (_key, val) in locked.entries() {
            if let Value::Number(n) = val {
                assert!(*n == 0.0 || *n == 1.0);
            } else {
                panic!("Expected number");
            }
        }
    } else {
        panic!("Expected array result");
    }
}

#[test]
fn test_markov_deterministic() {
    // Deterministic transition (always goes to state 1)
    let result = eval(r#"
        let matrix = [
            [0.0, 1.0],
            [0.0, 1.0]
        ];
        array_seq_markov(matrix, 0, 5);
    "#);

    if let Value::Array(arr) = result {
        let locked = arr.lock().unwrap();
        let entries = locked.entries();

        // First value should be 0 (start state)
        assert_eq!(entries[0].1, Value::Number(0.0));

        // All subsequent values should be 1
        for i in 1..entries.len() {
            assert_eq!(entries[i].1, Value::Number(1.0));
        }
    } else {
        panic!("Expected array result");
    }
}

#[test]
fn test_markov_three_states() {
    // Three-state Markov chain
    let result = eval(r#"
        seed(123);
        let matrix = [
            [0.5, 0.3, 0.2],
            [0.2, 0.5, 0.3],
            [0.3, 0.2, 0.5]
        ];
        array_seq_markov(matrix, 1, 15);
    "#);

    if let Value::Array(arr) = result {
        let locked = arr.lock().unwrap();
        assert_eq!(locked.len(), 15);

        // First value should be 1 (start state)
        let entries = locked.entries();
        assert_eq!(entries[0].1, Value::Number(1.0));

        // All values should be 0, 1, or 2
        for (_key, val) in entries {
            if let Value::Number(n) = val {
                assert!(*n >= 0.0 && *n <= 2.0);
            }
        }
    } else {
        panic!("Expected array result");
    }
}

#[test]
fn test_markov_reproducible() {
    // Same seed should produce same results
    let result1 = eval(r#"
        seed(999);
        let matrix = [
            [0.6, 0.4],
            [0.5, 0.5]
        ];
        array_seq_markov(matrix, 0, 20);
    "#);

    let result2 = eval(r#"
        seed(999);
        let matrix = [
            [0.6, 0.4],
            [0.5, 0.5]
        ];
        array_seq_markov(matrix, 0, 20);
    "#);

    assert_eq!(result1, result2);
}

// ---------------------------------------------------------------------------
// Composition Tests (Allowed, M Parts, Random)
// ---------------------------------------------------------------------------

#[test]
fn test_compositions_allowed_basic() {
    // Compositions of 5 using only 1s and 3s
    let result = eval(r#"array_seq_compositions_allowed(5, [1, 3]);"#);

    if let Value::Array(arr) = result {
        let locked = arr.lock().unwrap();
        // Should find compositions like [1,1,3], [1,3,1], [3,1,1], [1,1,1,1,1]
        assert!(locked.len() > 0);

        // Verify all compositions sum to 5 and use only allowed parts
        for (_key, comp_val) in locked.entries() {
            if let Value::Array(comp_arr) = comp_val {
                let comp_locked = comp_arr.lock().unwrap();
                let mut sum = 0.0;
                for (_k, v) in comp_locked.entries() {
                    if let Value::Number(n) = v {
                        sum += n;
                        assert!(*n == 1.0 || *n == 3.0, "Part {} not in allowed set", n);
                    }
                }
                assert_eq!(sum, 5.0);
            }
        }
    } else {
        panic!("Expected array result");
    }
}

#[test]
fn test_compositions_allowed_impossible() {
    // Compositions of 5 using only 2s - impossible
    let result = eval(r#"array_seq_compositions_allowed(5, [2]);"#);

    if let Value::Array(arr) = result {
        let locked = arr.lock().unwrap();
        assert_eq!(locked.len(), 0);
    } else {
        panic!("Expected array result");
    }
}

#[test]
fn test_compositions_m_parts_basic() {
    // Compositions of 5 into exactly 2 parts: [1,4], [2,3], [3,2], [4,1]
    let result = eval(r#"array_seq_compositions_m_parts(5, 2);"#);

    if let Value::Array(arr) = result {
        let locked = arr.lock().unwrap();
        assert_eq!(locked.len(), 4);

        // Verify all have exactly 2 parts and sum to 5
        for (_key, comp_val) in locked.entries() {
            if let Value::Array(comp_arr) = comp_val {
                let comp_locked = comp_arr.lock().unwrap();
                assert_eq!(comp_locked.len(), 2);

                let entries = comp_locked.entries();
                if let (Value::Number(a), Value::Number(b)) = (&entries[0].1, &entries[1].1) {
                    assert_eq!(a + b, 5.0);
                }
            }
        }
    } else {
        panic!("Expected array result");
    }
}

#[test]
fn test_compositions_m_parts_single_part() {
    // Compositions of 7 into 1 part: [7]
    let result = eval(r#"array_seq_compositions_m_parts(7, 1);"#);

    if let Value::Array(arr) = result {
        let locked = arr.lock().unwrap();
        assert_eq!(locked.len(), 1);

        let entries = locked.entries();
        if let Value::Array(comp_arr) = &entries[0].1 {
            let comp_locked = comp_arr.lock().unwrap();
            assert_eq!(comp_locked.len(), 1);
            let comp_entries = comp_locked.entries();
            assert_eq!(comp_entries[0].1, Value::Number(7.0));
        }
    } else {
        panic!("Expected array result");
    }
}

#[test]
fn test_compositions_allowed_m_parts_basic() {
    // Compositions of 8 into 3 parts using only 2s and 3s: [2,2,4] is invalid, [2,3,3], [3,2,3], [3,3,2]
    let result = eval(r#"array_seq_compositions_allowed_m_parts(8, 3, [2, 3]);"#);

    if let Value::Array(arr) = result {
        let locked = arr.lock().unwrap();
        // Should find [2,3,3], [3,2,3], [3,3,2]
        assert_eq!(locked.len(), 3);

        // Verify all have exactly 3 parts, sum to 8, and use only 2 or 3
        for (_key, comp_val) in locked.entries() {
            if let Value::Array(comp_arr) = comp_val {
                let comp_locked = comp_arr.lock().unwrap();
                assert_eq!(comp_locked.len(), 3);

                let mut sum = 0.0;
                for (_k, v) in comp_locked.entries() {
                    if let Value::Number(n) = v {
                        sum += n;
                        assert!(*n == 2.0 || *n == 3.0);
                    }
                }
                assert_eq!(sum, 8.0);
            }
        }
    } else {
        panic!("Expected array result");
    }
}

#[test]
fn test_compositions_allowed_m_parts_impossible() {
    // Compositions of 7 into 2 parts using only 3s - impossible (3+3=6, need 7)
    let result = eval(r#"array_seq_compositions_allowed_m_parts(7, 2, [3]);"#);

    if let Value::Array(arr) = result {
        let locked = arr.lock().unwrap();
        assert_eq!(locked.len(), 0);
    } else {
        panic!("Expected array result");
    }
}

#[test]
fn test_composition_random_basic() {
    // Random composition of 10
    let result = eval(r#"
        seed(42);
        array_seq_composition_random(10);
    "#);

    if let Value::Array(arr) = result {
        let locked = arr.lock().unwrap();
        assert!(locked.len() > 0);

        // Verify all parts are positive and sum to 10
        let mut sum = 0.0;
        for (_key, val) in locked.entries() {
            if let Value::Number(n) = val {
                assert!(*n > 0.0);
                sum += n;
            }
        }
        assert_eq!(sum, 10.0);
    } else {
        panic!("Expected array result");
    }
}

#[test]
fn test_composition_random_reproducible() {
    // Same seed should give same result
    let result1 = eval(r#"
        seed(999);
        array_seq_composition_random(15);
    "#);

    let result2 = eval(r#"
        seed(999);
        array_seq_composition_random(15);
    "#);

    assert_eq!(result1, result2);
}

#[test]
fn test_composition_random_m_parts_basic() {
    // Random composition of 20 into 5 parts
    let result = eval(r#"
        seed(123);
        array_seq_composition_random_m_parts(20, 5);
    "#);

    if let Value::Array(arr) = result {
        let locked = arr.lock().unwrap();
        assert_eq!(locked.len(), 5);

        // Verify all parts are positive and sum to 20
        let mut sum = 0.0;
        for (_key, val) in locked.entries() {
            if let Value::Number(n) = val {
                assert!(*n > 0.0);
                sum += n;
            }
        }
        assert_eq!(sum, 20.0);
    } else {
        panic!("Expected array result");
    }
}

#[test]
fn test_composition_random_m_parts_reproducible() {
    let result1 = eval(r#"
        seed(777);
        array_seq_composition_random_m_parts(30, 7);
    "#);

    let result2 = eval(r#"
        seed(777);
        array_seq_composition_random_m_parts(30, 7);
    "#);

    assert_eq!(result1, result2);
}

#[test]
fn test_composition_random_m_parts_edge_minimum() {
    // n=5, m=5 should give [1,1,1,1,1]
    let result = eval(r#"array_seq_composition_random_m_parts(5, 5);"#);

    if let Value::Array(arr) = result {
        let locked = arr.lock().unwrap();
        assert_eq!(locked.len(), 5);

        // All parts must be 1
        for (_key, val) in locked.entries() {
            assert_eq!(*val, Value::Number(1.0));
        }
    } else {
        panic!("Expected array result");
    }
}

// ---------------------------------------------------------------------------
// Continued Fraction Tests
// ---------------------------------------------------------------------------

#[test]
fn test_cf_convergent_basic() {
    // Continued fraction [1, 2, 2, 2, 2] approximates sqrt(2)
    // Should give convergent close to sqrt(2) ≈ 1.41421...
    let result = eval(r#"array_seq_cf_convergent([1, 2, 2, 2, 2]);"#);

    if let Value::Array(arr) = result {
        let locked = arr.lock().unwrap();
        assert_eq!(locked.len(), 2);

        let entries = locked.entries();
        if let (Value::Number(p), Value::Number(q)) = (&entries[0].1, &entries[1].1) {
            // 41/29 ≈ 1.41379...
            assert_eq!(*p, 41.0);
            assert_eq!(*q, 29.0);
        }
    } else {
        panic!("Expected array result");
    }
}

#[test]
fn test_cf_convergent_simple() {
    // [2] should give 2/1
    let result = eval(r#"array_seq_cf_convergent([2]);"#);

    if let Value::Array(arr) = result {
        let locked = arr.lock().unwrap();
        let entries = locked.entries();
        assert_eq!(entries[0].1, Value::Number(2.0));
        assert_eq!(entries[1].1, Value::Number(1.0));
    } else {
        panic!("Expected array result");
    }
}

#[test]
fn test_cf_convergent_golden_ratio() {
    // [1, 1, 1, 1, 1] approximates golden ratio φ ≈ 1.618...
    let result = eval(r#"array_seq_cf_convergent([1, 1, 1, 1, 1]);"#);

    if let Value::Array(arr) = result {
        let locked = arr.lock().unwrap();
        let entries = locked.entries();
        if let (Value::Number(p), Value::Number(q)) = (&entries[0].1, &entries[1].1) {
            // 8/5 = 1.6
            assert_eq!(*p, 8.0);
            assert_eq!(*q, 5.0);
        }
    } else {
        panic!("Expected array result");
    }
}

#[test]
fn test_cf_sqrt_perfect_square() {
    // sqrt(4) = 2, which is a perfect square
    let result = eval(r#"array_seq_cf_sqrt(4);"#);
    assert_eq!(result, Value::String("2 ( )".to_string()));
}

#[test]
fn test_cf_sqrt_non_perfect() {
    // sqrt(2) has continued fraction 1 + 1/(2 + 1/(2 + ...))
    // So it's "1 (2)" - the 2 repeats
    let result = eval(r#"array_seq_cf_sqrt(2);"#);
    assert_eq!(result, Value::String("1 ( 2 )".to_string()));
}

#[test]
fn test_cf_sqrt_seven() {
    // sqrt(7) = 2 + continued fraction
    let result = eval(r#"array_seq_cf_sqrt(7);"#);

    if let Value::String(s) = result {
        assert!(s.starts_with("2 ("));
        assert!(s.ends_with(" )"));
        assert!(s.contains("1 1 1 4")); // Period is (1, 1, 1, 4)
    } else {
        panic!("Expected string result");
    }
}

#[test]
fn test_cf_sqrt_three() {
    // sqrt(3) = 1 + continued fraction with period (1, 2)
    let result = eval(r#"array_seq_cf_sqrt(3);"#);
    assert_eq!(result, Value::String("1 ( 1 2 )".to_string()));
}

// ---------------------------------------------------------------------------
// Christoffel Word Tests
// ---------------------------------------------------------------------------

#[test]
fn test_christoffel_upper_basic() {
    // Upper Christoffel word for 3/5
    let result = eval(r#"array_seq_christoffel("upper", 3, 5, 8);"#);

    if let Value::String(s) = result {
        assert_eq!(s.len(), 8);
        // Should be a binary string
        assert!(s.chars().all(|c| c == '0' || c == '1'));
    } else {
        panic!("Expected string result");
    }
}

#[test]
fn test_christoffel_lower_basic() {
    // Lower Christoffel word for 3/5
    let result = eval(r#"array_seq_christoffel("lower", 3, 5, 8);"#);

    if let Value::String(s) = result {
        assert_eq!(s.len(), 8);
        assert!(s.chars().all(|c| c == '0' || c == '1'));
    } else {
        panic!("Expected string result");
    }
}

#[test]
fn test_christoffel_upper_vs_lower() {
    // Upper and lower should be different (complementary)
    let upper = eval(r#"array_seq_christoffel("upper", 2, 3, 5);"#);
    let lower = eval(r#"array_seq_christoffel("lower", 2, 3, 5);"#);

    assert_ne!(upper, lower);
}

#[test]
fn test_christoffel_default_length() {
    // Without specifying n, should default to p+q
    let result = eval(r#"array_seq_christoffel("upper", 3, 5);"#);

    if let Value::String(s) = result {
        assert_eq!(s.len(), 8); // 3 + 5 = 8
    } else {
        panic!("Expected string result");
    }
}

#[test]
fn test_christoffel_simple() {
    // For 1/2, upper should be "110"
    let result = eval(r#"array_seq_christoffel("upper", 1, 2);"#);

    if let Value::String(s) = result {
        assert_eq!(s.len(), 3);
        // Count ones and zeros
        let ones = s.chars().filter(|&c| c == '1').count();
        let zeros = s.chars().filter(|&c| c == '0').count();
        assert_eq!(ones, 1);
        assert_eq!(zeros, 2);
    } else {
        panic!("Expected string result");
    }
}

// ---------------------------------------------------------------------------
// Paper Folding Sequence Tests
// ---------------------------------------------------------------------------

#[test]
fn test_paper_folding_basic() {
    // Regular paper folding: n=7 (2^3-1), m=2, f=0
    let result = eval(r#"array_seq_paper_folding(7, 2, 0);"#);

    if let Value::String(s) = result {
        assert_eq!(s.len(), 7);
        assert!(s.chars().all(|c| c == '0' || c == '1'));
    } else {
        panic!("Expected string result");
    }
}

#[test]
fn test_paper_folding_length() {
    // Different lengths
    let result = eval(r#"array_seq_paper_folding(15, 3, 1);"#);

    if let Value::String(s) = result {
        assert_eq!(s.len(), 15);
    } else {
        panic!("Expected string result");
    }
}

#[test]
fn test_paper_folding_single() {
    // Single term
    let result = eval(r#"array_seq_paper_folding(1, 2, 0);"#);

    if let Value::String(s) = result {
        assert_eq!(s.len(), 1);
        assert!(s == "0" || s == "1");
    } else {
        panic!("Expected string result");
    }
}

#[test]
fn test_paper_folding_different_functions() {
    // Different f values should give different results
    let result1 = eval(r#"array_seq_paper_folding(7, 2, 0);"#);
    let result2 = eval(r#"array_seq_paper_folding(7, 2, 1);"#);
    let result3 = eval(r#"array_seq_paper_folding(7, 2, 2);"#);

    // They should not all be the same
    assert!(result1 != result2 || result2 != result3);
}

#[test]
fn test_paper_folding_deterministic() {
    // Same parameters should give same result
    let result1 = eval(r#"array_seq_paper_folding(15, 4, 5);"#);
    let result2 = eval(r#"array_seq_paper_folding(15, 4, 5);"#);

    assert_eq!(result1, result2);
}
