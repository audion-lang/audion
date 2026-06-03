mod common;

use audion::value::Value;

fn eval(src: &str) -> Value {
    common::eval(src)
}

fn num(n: f64) -> Value {
    Value::Number(n)
}

fn as_num(v: &Value) -> f64 {
    match v {
        Value::Number(n) => *n,
        _ => panic!("expected number, got {:?}", v),
    }
}

fn as_array(v: &Value) -> Vec<Value> {
    match v {
        Value::Array(arr) => {
            let locked = arr.lock().unwrap();
            locked.entries().iter().map(|(_, v)| v.clone()).collect()
        }
        _ => panic!("expected array, got {:?}", v),
    }
}

fn as_num_array(v: &Value) -> Vec<f64> {
    as_array(v).iter().map(|v| as_num(v)).collect()
}

// ===========================================================================
// ml_normalize
// ===========================================================================

#[test]
fn test_ml_normalize_basic() {
    let result = eval("ml_normalize([10, 30, 60]);");
    let nums = as_num_array(&result);
    assert_eq!(nums.len(), 3);
    assert!((nums[0] - 0.1).abs() < 1e-10);
    assert!((nums[1] - 0.3).abs() < 1e-10);
    assert!((nums[2] - 0.6).abs() < 1e-10);
}

#[test]
fn test_ml_normalize_sums_to_one() {
    let result = eval("ml_normalize([1, 2, 3, 4]);");
    let nums = as_num_array(&result);
    let sum: f64 = nums.iter().sum();
    assert!((sum - 1.0).abs() < 1e-10);
}

#[test]
fn test_ml_normalize_single() {
    let result = eval("ml_normalize([42]);");
    let nums = as_num_array(&result);
    assert_eq!(nums.len(), 1);
    assert!((nums[0] - 1.0).abs() < 1e-10);
}

// ===========================================================================
// ml_softmax
// ===========================================================================

#[test]
fn test_ml_softmax_sums_to_one() {
    let result = eval("ml_softmax([3.0, 1.0, 0.5]);");
    let nums = as_num_array(&result);
    let sum: f64 = nums.iter().sum();
    assert!((sum - 1.0).abs() < 1e-10);
}

#[test]
fn test_ml_softmax_ordering() {
    // Largest input should have largest probability
    let result = eval("ml_softmax([3.0, 1.0, 0.5]);");
    let nums = as_num_array(&result);
    assert!(nums[0] > nums[1]);
    assert!(nums[1] > nums[2]);
}

#[test]
fn test_ml_softmax_high_temperature_flattens() {
    let result_low = eval("ml_softmax([10.0, 0.0], 1.0);");
    let result_high = eval("ml_softmax([10.0, 0.0], 100.0);");
    let low = as_num_array(&result_low);
    let high = as_num_array(&result_high);
    // High temperature should make distribution more uniform
    assert!(high[1] > low[1]);
}

#[test]
fn test_ml_softmax_low_temperature_peaks() {
    let result = eval("ml_softmax([10.0, 0.0, 0.0], 0.1);");
    let nums = as_num_array(&result);
    // Very low temperature → first element should dominate
    assert!(nums[0] > 0.99);
}

#[test]
fn test_ml_softmax_equal_inputs() {
    let result = eval("ml_softmax([1.0, 1.0, 1.0]);");
    let nums = as_num_array(&result);
    // Equal inputs → uniform distribution
    for n in &nums {
        assert!((n - 1.0 / 3.0).abs() < 1e-10);
    }
}

// ===========================================================================
// ml_entropy
// ===========================================================================

#[test]
fn test_ml_entropy_uniform() {
    // Uniform distribution over 4 → entropy = 2.0 bits
    let result = eval("ml_entropy([0.25, 0.25, 0.25, 0.25]);");
    let h = as_num(&result);
    assert!((h - 2.0).abs() < 1e-10);
}

#[test]
fn test_ml_entropy_certain() {
    // All probability on one outcome → entropy = 0
    let result = eval("ml_entropy([1.0, 0.0, 0.0]);");
    let h = as_num(&result);
    assert!((h - 0.0).abs() < 1e-10);
}

#[test]
fn test_ml_entropy_binary() {
    // Fair coin → entropy = 1.0 bit
    let result = eval("ml_entropy([0.5, 0.5]);");
    let h = as_num(&result);
    assert!((h - 1.0).abs() < 1e-10);
}

#[test]
fn test_ml_entropy_skewed() {
    // Skewed distribution → lower entropy than uniform
    let uniform = as_num(&eval("ml_entropy([0.25, 0.25, 0.25, 0.25]);"));
    let skewed = as_num(&eval("ml_entropy([0.7, 0.1, 0.1, 0.1]);"));
    assert!(skewed < uniform);
}

// ===========================================================================
// ml_weighted_choice
// ===========================================================================

#[test]
fn test_ml_weighted_choice_deterministic() {
    // Weight 0 on everything except one → always picks that one
    let result = eval(r#"
        seed(42);
        let results = [];
        for (let i = 0; i < 20; i += 1) {
            push(results, ml_weighted_choice([10, 20, 30], [0, 0, 1]));
        }
        results;
    "#);
    let nums = as_num_array(&result);
    for n in &nums {
        assert_eq!(*n, 30.0);
    }
}

#[test]
fn test_ml_weighted_choice_distribution() {
    // Heavily weighted toward first element
    let result = eval(r#"
        seed(123);
        let counts = [0, 0, 0];
        for (let i = 0; i < 1000; i += 1) {
            let pick = ml_weighted_choice([0, 1, 2], [0.8, 0.1, 0.1]);
            counts[pick] = counts[pick] + 1;
        }
        counts;
    "#);
    let counts = as_num_array(&result);
    // First element should dominate
    assert!(counts[0] > counts[1]);
    assert!(counts[0] > counts[2]);
    assert!(counts[0] > 600.0); // should be ~800 but allow variance
}

#[test]
fn test_ml_weighted_choice_strings() {
    let result = eval(r#"
        seed(42);
        ml_weighted_choice(["kick", "snare", "hat"], [0, 0, 1]);
    "#);
    assert_eq!(result, Value::String("hat".to_string()));
}

// ===========================================================================
// ml_markov_train — basic training
// ===========================================================================

#[test]
fn test_ml_markov_train_order1() {
    let result = eval(r#"
        let notes = [60, 62, 64, 62, 60];
        let model = ml_markov_train(notes, 1);
        model[0];
    "#);
    // model[0] is the order
    assert_eq!(result, num(1.0));
}

#[test]
fn test_ml_markov_train_order2() {
    let result = eval(r#"
        let notes = [60, 62, 64, 67, 64, 62, 60];
        let model = ml_markov_train(notes, 2);
        model[0];
    "#);
    assert_eq!(result, num(2.0));
}

#[test]
fn test_ml_markov_train_has_transitions() {
    // After training, model[1] should be a non-empty array
    let result = eval(r#"
        let notes = [60, 62, 64, 62, 60];
        let model = ml_markov_train(notes, 1);
        count(model[1]) > 0;
    "#);
    assert_eq!(result, Value::Bool(true));
}

// ===========================================================================
// ml_markov_generate — generation from trained model
// ===========================================================================

#[test]
fn test_ml_markov_generate_length() {
    let result = eval(r#"
        seed(42);
        let notes = [60, 62, 64, 67, 64, 62, 60, 62, 64, 67, 64, 62];
        let model = ml_markov_train(notes, 1);
        let melody = ml_markov_generate(model, 20);
        count(melody);
    "#);
    assert_eq!(result, num(20.0));
}

#[test]
fn test_ml_markov_generate_values_in_range() {
    // Generated values should be from the training corpus
    let result = eval(r#"
        seed(42);
        let notes = [60, 62, 64, 67];
        let model = ml_markov_train(notes, 1);
        let melody = ml_markov_generate(model, 50);
        let all_valid = true;
        for (let i = 0; i < count(melody); i += 1) {
            let n = melody[i];
            if (n != 60 && n != 62 && n != 64 && n != 67) {
                all_valid = false;
            }
        }
        all_valid;
    "#);
    assert_eq!(result, Value::Bool(true));
}

#[test]
fn test_ml_markov_generate_reproducible() {
    // Use explicit start context to avoid HashMap iteration order differences
    let result1 = eval(r#"
        seed(42);
        let notes = [60, 62, 64, 67, 64, 62, 60, 62, 64, 67];
        let model = ml_markov_train(notes, 1);
        ml_markov_generate(model, 10, [60]);
    "#);
    let result2 = eval(r#"
        seed(42);
        let notes = [60, 62, 64, 67, 64, 62, 60, 62, 64, 67];
        let model = ml_markov_train(notes, 1);
        ml_markov_generate(model, 10, [60]);
    "#);
    assert_eq!(as_num_array(&result1), as_num_array(&result2));
}

#[test]
fn test_ml_markov_generate_order2() {
    let result = eval(r#"
        seed(42);
        let notes = [60, 62, 64, 67, 64, 62, 60, 62, 64, 67, 64, 62, 60];
        let model = ml_markov_train(notes, 2);
        let melody = ml_markov_generate(model, 16);
        count(melody);
    "#);
    assert_eq!(result, num(16.0));
}

#[test]
fn test_ml_markov_generate_with_start() {
    let result = eval(r#"
        seed(42);
        let notes = [60, 62, 64, 67, 64, 62, 60, 62, 64, 67, 64, 62];
        let model = ml_markov_train(notes, 2);
        let melody = ml_markov_generate(model, 10, [60, 62]);
        count(melody);
    "#);
    assert_eq!(result, num(10.0));
}

#[test]
fn test_ml_markov_generate_deterministic_chain() {
    // Train on a repeating pattern → generation should follow the pattern
    let result = eval(r#"
        seed(42);
        let notes = [1, 2, 3, 1, 2, 3, 1, 2, 3, 1, 2, 3];
        let model = ml_markov_train(notes, 2);
        let melody = ml_markov_generate(model, 9, [1, 2]);
        melody;
    "#);
    let nums = as_num_array(&result);
    // With order-2 on [1,2,3] repeating, after [1,2] → 3, after [2,3] → 1, etc.
    assert_eq!(nums[0], 3.0);
    assert_eq!(nums[1], 1.0);
    assert_eq!(nums[2], 2.0);
    assert_eq!(nums[3], 3.0);
    assert_eq!(nums[4], 1.0);
    assert_eq!(nums[5], 2.0);
}

// ===========================================================================
// ml_markov_next — probability distribution query
// ===========================================================================

#[test]
fn test_ml_markov_next_returns_distribution() {
    let result = eval(r#"
        let notes = [60, 62, 64, 62, 60, 62, 67];
        let model = ml_markov_train(notes, 1);
        let probs = ml_markov_next(model, [62]);
        count(probs) > 0;
    "#);
    assert_eq!(result, Value::Bool(true));
}

#[test]
fn test_ml_markov_next_probabilities_sum_to_one() {
    let result = eval(r#"
        let notes = [60, 62, 64, 62, 60, 62, 67, 60];
        let model = ml_markov_train(notes, 1);
        let probs = ml_markov_next(model, [62]);
        let sum = 0;
        for (let i = 0; i < count(probs); i += 1) {
            sum = sum + probs[i][1];
        }
        sum;
    "#);
    let sum = as_num(&result);
    assert!((sum - 1.0).abs() < 1e-10);
}

#[test]
fn test_ml_markov_next_deterministic() {
    // After 1 always comes 2 in this sequence
    let result = eval(r#"
        let notes = [1, 2, 3, 1, 2, 3, 1, 2, 3];
        let model = ml_markov_train(notes, 1);
        let probs = ml_markov_next(model, [1]);
        // Should be [[2, 1.0]] — 100% chance of 2 after 1
        probs[0][0];
    "#);
    assert_eq!(result, num(2.0));
}

#[test]
fn test_ml_markov_next_empty_for_unknown_context() {
    let result = eval(r#"
        let notes = [60, 62, 64];
        let model = ml_markov_train(notes, 1);
        let probs = ml_markov_next(model, [99]);
        count(probs);
    "#);
    assert_eq!(result, num(0.0));
}

// ===========================================================================
// ml_markov_train + generate — higher order tests
// ===========================================================================

#[test]
fn test_ml_markov_order3() {
    let result = eval(r#"
        seed(42);
        let notes = [1, 2, 3, 4, 1, 2, 3, 4, 1, 2, 3, 4, 1, 2, 3, 4];
        let model = ml_markov_train(notes, 3);
        let melody = ml_markov_generate(model, 8, [1, 2, 3]);
        melody;
    "#);
    let nums = as_num_array(&result);
    // [1,2,3] → 4, [2,3,4] → 1, etc.
    assert_eq!(nums[0], 4.0);
    assert_eq!(nums[1], 1.0);
    assert_eq!(nums[2], 2.0);
    assert_eq!(nums[3], 3.0);
}

// ===========================================================================
// ml_markov — backoff behavior
// ===========================================================================

#[test]
fn test_ml_markov_backoff_high_order() {
    // Order 5 on a short corpus — most 5-note contexts are unique.
    // Without backoff this would hit dead ends and sound random.
    // With backoff it should still produce notes from the corpus.
    let result = eval(r#"
        seed(42);
        let notes = [60, 62, 64, 67, 64, 62, 60, 64, 67, 72, 67, 64, 60];
        let model = ml_markov_train(notes, 5);
        let melody = ml_markov_generate(model, 30);
        let all_valid = true;
        for (let i = 0; i < count(melody); i += 1) {
            let n = melody[i];
            if (n != 60 && n != 62 && n != 64 && n != 67 && n != 72) {
                all_valid = false;
            }
        }
        all_valid;
    "#);
    assert_eq!(result, Value::Bool(true));
}

#[test]
fn test_ml_markov_backoff_model_has_all_orders() {
    // Order-3 model should have 4 entries: [order, trans_3, trans_2, trans_1]
    let result = eval(r#"
        let notes = [1, 2, 3, 4, 1, 2, 3, 4, 1, 2, 3, 4];
        let model = ml_markov_train(notes, 3);
        count(model);
    "#);
    assert_eq!(result, num(4.0)); // order + 3 transition tables
}

#[test]
fn test_ml_markov_backoff_generates_full_length() {
    // Even with absurdly high order, backoff ensures we always get the requested length
    let result = eval(r#"
        seed(42);
        let notes = [60, 62, 64, 67, 64, 62, 60, 62, 64, 67, 64, 62, 60, 62, 64];
        let model = ml_markov_train(notes, 10);
        let melody = ml_markov_generate(model, 50);
        count(melody);
    "#);
    assert_eq!(result, num(50.0));
}

// ===========================================================================
// Error cases
// ===========================================================================

#[test]
fn test_ml_normalize_error_empty() {
    let result = std::panic::catch_unwind(|| eval("ml_normalize([]);"));
    assert!(result.is_err());
}

#[test]
fn test_ml_normalize_error_zero_sum() {
    let result = std::panic::catch_unwind(|| eval("ml_normalize([0, 0, 0]);"));
    assert!(result.is_err());
}

#[test]
fn test_ml_softmax_error_empty() {
    let result = std::panic::catch_unwind(|| eval("ml_softmax([]);"));
    assert!(result.is_err());
}

#[test]
fn test_ml_entropy_error_empty() {
    let result = std::panic::catch_unwind(|| eval("ml_entropy([]);"));
    assert!(result.is_err());
}

#[test]
fn test_ml_weighted_choice_error_mismatch() {
    let result = std::panic::catch_unwind(|| eval("ml_weighted_choice([1, 2], [0.5]);"));
    assert!(result.is_err());
}

#[test]
fn test_ml_markov_train_error_order_zero() {
    let result = std::panic::catch_unwind(|| eval("ml_markov_train([1, 2, 3], 0);"));
    assert!(result.is_err());
}

#[test]
fn test_ml_markov_train_error_too_short() {
    let result = std::panic::catch_unwind(|| eval("ml_markov_train([1], 1);"));
    assert!(result.is_err());
}
