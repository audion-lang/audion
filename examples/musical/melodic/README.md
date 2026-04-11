# Audion Melodic Generation Examples

This directory contains examples demonstrating Audion's melodic generation functions for algorithmic composition and generative music.

## Overview

Audion provides several approaches to melodic generation, all following the `array_mel_*` naming convention:

### K-ary De Bruijn Sequences
- `array_mel_debruijn_k(k, n, v)` - Generates k-ary de Bruijn sequences
- Like binary de Bruijn but uses k symbols (not just 0/1)
- **Musical use:** Generate all possible n-note patterns using k scale degrees

### Lattice Walks
- `array_mel_lattice_walk_square(nc, nr, x, y, a, b, n)` - Walks on square grids
- `array_mel_lattice_walk_tri(nc, nr, x, y, a, b, n)` - Walks on triangular grids
- **Musical use:** Map spatial paths to melodic contours

**Directions:**
- Square: `r` (right), `l` (left), `u` (up), `d` (down)
- Triangular: adds `v` (diagonal ↘), `e` (diagonal ↖)

### String to Melody
- `array_mel_string_to_indices(string, num_notes)` - Maps text to note indices
- Digits '0'-'9' → 0-9, lowercase 'a'-'z' → 10-35, uppercase 'A'-'Z' → 36-61
- **Musical use:** Turn names, words, hex values into melodies

### Random Walk
- `array_mel_random_walk(start, min, max, step_size, length)` - Bounded random walk
- Generates melodic lines that drift within a range
- **Musical use:** Organic, wandering melodies with controlled boundaries

### Transformations
- `array_mel_invert(melody, pivot)` - Melodic inversion around a pivot note
- `array_mel_reverse(melody)` - Retrograde (reverse the melody)
- **Musical use:** Classical transformation techniques (12-tone, serialism)

## Examples

### 01_random_walk.au
**Bounded random pitch generation**

Demonstrates:
- `array_mel_random_walk()` for organic melodies
- Different step sizes create different characters
- Small steps = smooth, large steps = jumpy

```javascript
let smooth = array_mel_random_walk(60, 55, 65, 0.5, 16);   // Gentle
let jumpy = array_mel_random_walk(60, 48, 72, 5, 16);      // Dramatic
```

### 02_lattice_melodies.au
**Map grid walks to note sequences**

Demonstrates:
- `array_mel_lattice_walk_square()` generates paths
- Converting direction strings to melodic intervals
- Spatial thinking → musical thinking

Key insight: Different paths = different melodies, but all have the same "distance"

### 03_string_melodies.au
**Convert text to musical patterns**

Demonstrates:
- `array_mel_string_to_indices()` for text-to-music
- Mapping indices to actual scales
- Creative uses: names, hex values, words become melodies

```javascript
let pattern = array_mel_string_to_indices("cafebabe", 7);  // Hex → melody
let scale = [60, 62, 64, 65, 67, 69, 71];  // C major
// Map indices to actual notes from the scale
```

### 04_transformations.au
**Invert and reverse melodies**

Demonstrates:
- `array_mel_invert()` for melodic inversion
- `array_mel_reverse()` for retrograde
- Combining transformations (retrograde inversion)
- Classic 12-tone techniques

Musical transformations:
- **Original:** The melody as written
- **Inversion:** Intervals flipped around a pivot
- **Retrograde:** Played backwards
- **Retrograde Inversion:** Backwards + inverted

### 05_debruijn_scales.au
**Generate all possible note patterns**

Demonstrates:
- `array_mel_debruijn_k()` for exhaustive pattern generation
- Converting digit sequences to melodies
- Using pentatonic and triadic scales

Example: k=5, n=2 generates all possible 2-note patterns using 5 scale degrees

## Musical Concepts

### Lattice Walks as Melodies
A lattice is a grid of points. Walking on it creates a path:
- **Square lattice:** 4 directions (like chess rook)
- **Triangular lattice:** 6 directions (includes diagonals)

Map directions to intervals:
- Right → up 2 semitones (whole step)
- Left → down 2 semitones
- Up → up 1 semitone (half step)
- Down → down 1 semitone
- Diagonals → thirds or other intervals

### De Bruijn for Melodic Patterns
A k-ary de Bruijn sequence of order n contains every possible n-length pattern of k symbols exactly once.

**Musical example:**
- k=3 (three notes: C, E, G)
- n=2 (looking at pairs of notes)
- Result: a sequence containing all 9 possible pairs: CC, CE, CG, EC, EE, EG, GC, GE, GG

Perfect for exploring all melodic possibilities in a constrained space.

### Melodic Transformations
Classical composition techniques:

1. **Inversion:** Flip intervals around a pivot
   - Original: C-E-G (up 4, up 3)
   - Inverted around C: C-Ab-F (down 4, down 3)

2. **Retrograde:** Play backwards
   - Original: C-D-E-F
   - Retrograde: F-E-D-C

3. **Retrograde Inversion:** Both at once
   - A staple of 12-tone and serial music

### Random Walks
A random walk with boundaries:
- Starts at a point
- Each step: random movement
- Stays within min/max bounds
- Reflects or clamps at boundaries

**Step size controls character:**
- Small steps (0.5-1): Smooth, vocal-like
- Medium steps (2-3): Natural melodic motion
- Large steps (5+): Dramatic, angular

## Running the Examples

```bash
# Run any example directly
audion run examples/musical/melodic/01_random_walk.au

# Or from the REPL
audion
> include "examples/musical/melodic/01_random_walk.au";
```

## Creative Tips

### 1. Combine with Sequences
Use `array_seq_*` for rhythm, `array_mel_*` for pitch:
```javascript
let rhythm = array_seq_euclidean(5, 8);
let melody = array_mel_random_walk(60, 48, 72, 2, 8);
// Play melody notes only when rhythm has hits
```

### 2. Transform Melodies Over Time
```javascript
let original = [60, 62, 64, 65];
let inverted = array_mel_invert(original, 60);
let reversed = array_mel_reverse(original);
// Morph between versions
```

### 3. Use Lattice Walks for Contour
```javascript
// Generate a walk, map to melody
let walk = array_mel_lattice_walk_square(4, 4, 0, 0, 3, 3, 8);
// Each direction becomes an interval
```

### 4. String Encoding
```javascript
// Encode your name as a melody!
let name_melody = array_mel_string_to_indices("alice", 7);
// Map to your favorite scale
```

### 5. De Bruijn for Exploration
```javascript
// Exhaustively explore all 3-note patterns in a pentatonic scale
let db = array_mel_debruijn_k(5, 3, 0);
// Contains every possible triplet exactly once
```

### 6. Seed for Reproducibility
```javascript
seed(42);  // Same random walks every time
let walk = array_mel_random_walk(60, 48, 72, 2, 16);
```

## Comparing Melodic vs Sequence Functions

| Aspect | `array_seq_*` | `array_mel_*` |
|--------|---------------|---------------|
| Purpose | Rhythm, timing, patterns | Pitch, melody, transformations |
| Output | Binary, intervals, partitions | Walks, scales, contours |
| Musical Use | When to play | What to play |
| Combine? | **YES!** Use both together for complete music |

**Example combination:**
```javascript
// Euclidean rhythm
let hits = array_seq_euclidean(5, 8);

// Random walk melody
let pitches = array_mel_random_walk(60, 48, 72, 2, 8);

// Play pitches only on hits
```

## Advanced Techniques

### Markov Chains + Lattice Walks
1. Use lattice walks to generate possible melodic shapes
2. Build a Markov chain from those shapes
3. Generate new melodies probabilistically

### Serial Techniques
```javascript
// Create a tone row
let row = [60, 63, 67, 70, 62, 65, 69, 72, 61, 64, 68, 71];

// Four classic transformations
let prime = row;
let inversion = array_mel_invert(row, 60);
let retrograde = array_mel_reverse(row);
let retro_inv = array_mel_reverse(inversion);
```

### Constrained Randomness
```javascript
// Walk that prefers certain ranges
let walk = array_mel_random_walk(60, 48, 72, 2, 32);

// Extract only notes in upper register
let high_notes = [];
for note in walk {
    if (note > 65) { array_push(high_notes, note); }
}
```

## Further Reading

- **Lattice Walks:** Used in combinatorics and music theory (Xenakis, mathematical composition)
- **De Bruijn Sequences:** Information theory, cryptography, and algorithmic composition
- **Melodic Transformations:** 12-tone technique (Schoenberg), serial music
- **Random Walks:** Stochastic processes, brownian motion in music (Xenakis)
- **Text-to-Music:** Gematria, numerology, encoding in classical music

## Next Steps

- Combine melodic and rhythmic functions
- Build longer compositions from generated phrases
- Create your own scales and mappings
- Explore lattice walks with different interval mappings
- Try different pivot points for inversions
- Generate tone rows and apply serial techniques

🎵🎵🎵🎵
