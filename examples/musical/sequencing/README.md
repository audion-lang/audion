# Audion Sequencing Examples

This directory contains examples demonstrating Audion's sequence generation functions for algorithmic composition and generative music.

## Overview

Audion provides several mathematical and algorithmic approaches to sequence generation, all following the `array_seq_*` naming convention:

### Euclidean Rhythms
- `array_seq_euclidean(pulses, steps)` - Distributes pulses evenly across steps
- Based on Godfried Toussaint's work connecting the Euclidean algorithm to traditional rhythms
- Examples: `(5, 8)` produces the Cuban tresillo, `(3, 8)` produces the standard backbeat

### Integer Partitions
- `array_seq_partitions(n)` - All unordered ways to sum to n
- `array_seq_partitions_allowed(n, allowed)` - Using only specific values
- `array_seq_partitions_m_parts(n, m)` - Exactly m parts
- `array_seq_partitions_allowed_m_parts(n, m, allowed)` - Combined constraints

**Musical use:** Time division, polyrhythms, grouping structures

### Integer Compositions
- `array_seq_compositions(n)` - All **ordered** ways to sum to n
- Like partitions but order matters: `[1,2]` ≠ `[2,1]`

**Musical use:** Rhythmic phrasing, melodic contours, duration sequences

### Binary Necklaces
- `array_seq_necklaces(n)` - Rotation-invariant binary patterns
- `array_seq_necklaces_allowed(n, allowed)` - With run length constraints
- `array_seq_necklaces_m_ones(n, m)` - Exactly m ones (hits)
- `array_seq_necklaces_allowed_m_ones(n, m, allowed)` - Combined constraints

**Musical use:** Cyclic rhythms, drum patterns, repeating motifs

### Markov Chains
- `array_seq_markov(matrix, start_state, count)` - Probabilistic sequences
- Matrix[i][j] = probability of transitioning from state i to state j

**Musical use:** Melodic generation, harmony progressions, rhythmic variation

### Other Functions
- `array_seq_binary_to_intervals(binary_string)` - Convert "1010010" to intervals [2,3,4,...]
- `array_seq_intervals_to_binary(intervals)` - Reverse conversion
- `array_seq_random_correlated(m, s, c, n)` - Random with correlation control
- `array_seq_permutations(array)` - All orderings of elements
- `array_seq_debruijn(n)` - Maximal information density sequences

## Examples

### 01_euclidean_rhythms.au
**Basic Euclidean patterns for drums**

Demonstrates:
- `array_seq_euclidean()` for kick, snare, hi-hat
- How to combine multiple patterns
- Classic techno/house drum programming

```javascript
let kick_pattern = array_seq_euclidean(4, 16);   // Four-on-the-floor
let snare_pattern = array_seq_euclidean(3, 8);   // Tresillo
let hat_pattern = array_seq_euclidean(5, 8);     // Dense hi-hats
```

### 02_partition_polyrhythms.au
**Integer partitions for rhythmic variety**

Demonstrates:
- `array_seq_partitions_m_parts()` for time division
- `array_seq_partitions_allowed_m_parts()` for constrained patterns
- Using partition values as durations

Explores all ways to divide 8 beats among 3 drums:
- `[1,1,6]` - sparse + long tail
- `[2,2,4]` - even distribution
- `[1,3,4]` - asymmetric grouping

### 03_necklace_patterns.au
**Binary necklaces for cyclic patterns**

Demonstrates:
- `array_seq_necklaces()` generates rotation-invariant sequences
- `array_seq_necklaces_m_ones()` controls density (hits per cycle)
- Converting binary strings to playable patterns

Necklaces are perfect for:
- Drum loops (automatically avoid redundant rotations)
- Cyclic melodies
- Repeating motifs

### 04_markov_melody.au
**Probabilistic melody generation**

Demonstrates:
- `array_seq_markov()` with transition matrix
- Creating melodic tendencies (stepwise motion, leaps, anchors)
- Different matrices = different styles
- Using `seed()` for reproducibility

Transition matrix example:
```javascript
let matrix = [
    [0.3, 0.4, 0.2, 0.1, 0.0],  // From C
    [0.2, 0.2, 0.4, 0.1, 0.1],  // From D
    // ... etc
];
```

High diagonal = stay on same note
High adjacent values = stepwise motion
Uniform distribution = random walk

### 05_generative_drums.au
**Combining multiple techniques**

Demonstrates:
- Using Euclidean + necklaces + partitions together
- Creating complementary patterns
- Varying patterns over time
- Building complete drum arrangements

Pattern layers:
- **Kick:** Euclidean 4/16 (steady pulse)
- **Snare:** Necklace with 2/8 (varied backbeat)
- **Hi-hat:** Euclidean 7/8 (dense, syncopated)
- **Perc:** Partition-based accents

### 06_compositions_rhythm.au
**Ordered vs unordered subdivision**

Demonstrates:
- `array_seq_compositions()` for **ordered** sequences
- Comparing compositions (order matters) vs partitions (unordered)
- Using composition values as note durations
- Creating accented rhythmic phrases

Key insight:
- Partition `{1,1,2}` has 3 orderings: `[1,1,2]`, `[2,1,1]`, `[1,2,1]`
- Each creates a different rhythmic feel despite same durations

## Running the Examples

```bash
# Run any example directly
audion run examples/musical/sequencing/01_euclidean_rhythms.au

# Or from the REPL
audion
> include "examples/musical/sequencing/01_euclidean_rhythms.au";
```

## Musical Concepts

### Euclidean Rhythms in World Music
- `(5, 8)` = Cuban tresillo, West African timeline
- `(3, 4)` = Standard rock beat
- `(5, 12)` = York Samai (Middle Eastern)
- `(7, 16)` = Brazilian Bossa Nova
- `(5, 9)` = Arab rhythms

### Partitions vs Compositions
- **Partition:** Unordered set, fewer results
  - `4 = {4}, {3,1}, {2,2}, {2,1,1}, {1,1,1,1}`
  - 5 partitions
- **Composition:** Ordered sequence, more results
  - `4 = [4], [3,1], [1,3], [2,2], [2,1,1], [1,2,1], [1,1,2], [1,1,1,1]`
  - 8 compositions

Use partitions for grouping/texture, compositions for melodic/rhythmic phrasing.

### Necklaces for Rhythm Design
A necklace is a circular pattern where rotations are considered identical:
- `"001"` = `"010"` = `"100"` (all rotations of same necklace)
- Only one representative is generated

Perfect for drum loops: ensures pattern variety without redundant rotations.

### Markov Chains for Style
Design your transition matrix to create different musical styles:
- **Stepwise:** High probabilities for adjacent states
- **Anchored:** High diagonal (tendency to repeat)
- **Random walk:** Uniform probabilities
- **Structured:** Encode scale degree tendencies (e.g., V→I attraction)

## Tips for Creative Use

1. **Seed for reproducibility:** Use `seed(123)` before generating sequences if you want the same results each time

2. **Combine techniques:**
   - Euclidean for main rhythm
   - Necklaces for variations
   - Markov for melodic content
   - Partitions for polyrhythmic layers

3. **Use constraints wisely:**
   - `allowed` parameters filter results
   - Constraining to powers of 2 creates metric feel
   - Prime numbers create asymmetry

4. **Convert between representations:**
   - Binary strings ↔ interval notation
   - State numbers ↔ MIDI notes
   - Partitions ↔ duration values

5. **Iterate and explore:**
   - Generate all patterns, then cherry-pick
   - Use randomness to select from generated sets
   - Morph between patterns over time

## Further Reading

- **Euclidean Rhythms:** "The Euclidean Algorithm Generates Traditional Musical Rhythms" by Godfried Toussaint
- **Necklaces:** Used in combinatorics, cycle theory, and Lyndon words
- **Markov Chains:** First-order Markov models in algorithmic composition
- **Partitions:** Integer partition theory (Hardy, Ramanujan)

## Next Steps

- Experiment with different parameter values
- Combine multiple sequence types
- Create your own transition matrices
- Build longer compositions from generated phrases
- Add effects and dynamics
- Explore higher-order Markov chains (longer memory)

Happy sequencing! 🎵
