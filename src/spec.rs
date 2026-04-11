// Copyright (C) 2025-2026 Aleksandr Bogdanov
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.
//
// You should have received a copy of the GNU General Public License
// along with this program.  If not, see <https://www.gnu.org/licenses/>.
//
//

use crate::builtins::BUILTIN_NAMES;
use crate::lexer::KEYWORDS;
use crate::synthdef::{DEFAULT_PARAMS, UGEN_NAMES};

fn backtick_join(names: &[&str]) -> String {
    names
        .iter()
        .map(|n| format!("`{}`", n))
        .collect::<Vec<_>>()
        .join(" ")
}

/// Generate the compact AGENTS.md specification from source constants.
pub fn generate() -> String {
    let mut out = String::new();

    out.push_str("# Audion Language Spec (auto-generated)\n\n");
    out.push_str("<!-- regenerate: audion spec > docs/AGENTS.md -->\n\n");

    // -- Language overview --------------------------------------------------
    out.push_str("## Overview\n\n");
    out.push_str("Audion is a general purpose language for audiovisual art, performance and installation\n");
    out.push_str("It targets SuperCollider via OSC. File extension: `.au`\n\n");

    // -- Types --------------------------------------------------------------
    out.push_str("## Types\n\n");
    out.push_str(concat!(
        "number (f64), string (double-quoted), bool, nil, ",
        "function (first-class, closures), array (ordered key-value), ",
        "object (`return this;`), namespace (`include`)\n\n",
    ));
    out.push_str("Falsy: `nil`, `false`, `0`, `\"\"`, `[]`, empty object. Everything else truthy.\n\n");

    // -- Keywords (from lexer::KEYWORDS) ------------------------------------
    out.push_str("## Keywords\n\n");
    out.push_str(&backtick_join(KEYWORDS));
    out.push_str("\n\n");

    // -- Operators ----------------------------------------------------------
    out.push_str("## Operators (by precedence, low→high)\n\n");
    out.push_str(concat!(
        "1. `= += -= *= /=`\n",
        "2. `||`\n",
        "3. `&&`\n",
        "4. `|`\n",
        "5. `^`\n",
        "6. `&`\n",
        "7. `== !=`\n",
        "8. `< > <= >=`\n",
        "9. `<< >>`\n",
        "10. `+ -`\n",
        "11. `* / %`\n",
        "12. `- ! ~` (unary)\n",
        "13. calls, `[]`, `.`\n\n",
    ));

    // -- Syntax quick-ref ---------------------------------------------------
    out.push_str("## Syntax\n\n");
    out.push_str("```\n");
    out.push_str(concat!(
        "let x = 5;                          // variable\n",
        "x = 10;                             // assign (creates if not found)\n",
        "let a = [1, 2, \"k\" => 3];           // array (PHP-style ordered kv)\n",
        "fn add(a, b) { return a + b; }      // function decl\n",
        "let f = fn(x) { return x * 2; };    // anonymous fn / closure\n",
        "synth(\"name\", freq: 440, amp: 0.5); // named args\n",
        "if (c) { } else { }                 // control flow\n",
        "while (c) { }                       // while loop\n",
        "loop { break; }                     // infinite loop\n",
        "for (let i=0; i<10; i+=1) { }       // C-style for\n",
        "thread drums { loop { wait(1); } }  // named thread\n",
        "include \"lib.au\";                    // include file\n",
        "include \"lib.au\" as utils;           // aliased include\n",
        "using lib;                           // import namespace\n",
        "// line comment  /* block comment */\n",
    ));
    out.push_str("```\n\n");

    // -- Objects ------------------------------------------------------------
    out.push_str("## Objects\n\n");
    out.push_str("```\n");
    out.push_str(concat!(
        "fn make_obj() {\n",
        "    let val = 0;\n",
        "    let get = fn() { return val; };\n",
        "    return this;    // captures scope as object\n",
        "}\n",
        "let o = make_obj();\n",
        "o.val = 42;         // field access via dot\n",
        "o.get();            // method call\n",
    ));
    out.push_str("```\n\n");

    // -- SynthDef -----------------------------------------------------------
    out.push_str("## SynthDef (define blocks)\n\n");
    out.push_str("```\n");
    out.push_str(concat!(
        "define bass(freq, amp, gate, out) {\n",
        "    out(out, lpf(saw(freq), freq * 4) * env(gate) * amp);\n",
        "}\n",
        "synth(\"bass\", freq: 110, amp: 0.5);\n",
        "// look in the examples folder for more\n",
    ));
    out.push_str("```\n\n");

    // -- Builtins (from builtins::BUILTIN_NAMES) ----------------------------
    out.push_str("## Builtins\n\n");

    // Prefix-based categories. Each builtin is assigned to the category
    // with the longest matching prefix (so array_seq_ wins over array_).
    // "core" is the automatic catch-all for anything with no prefix match.
    let prefixed: &[(&str, &str)] = &[
        ("sequence", "array_seq_"),
        ("melody",   "array_mel_"),
        ("array",    "array_"),
        ("buffer",   "buffer_"),
        ("console",  "console_"),
        ("database", "sqlite_"),
        ("dir",      "dir_"),
        ("file",     "file_"),
        ("json",     "json_"),
        ("link",     "link_"),
        ("math",     "math_"),
        ("midi",     "midi_"),
        ("net",      "net_"),
        ("osc",      "osc_"),
        ("os",       "os_"),
        ("string",   "str"),
        ("date",     "date"),
        ("date",     "timestamp"),
    ];

    // Find the longest matching prefix for a builtin name
    let best_category = |n: &str| -> Option<&str> {
        prefixed
            .iter()
            .filter(|(_, pfx)| n.starts_with(pfx))
            .max_by_key(|(_, pfx)| pfx.len())
            .map(|(label, _)| *label)
    };

    // Collect category labels in display order (preserving first occurrence)
    let mut seen_labels = Vec::new();
    for (label, _) in prefixed {
        if !seen_labels.contains(label) {
            seen_labels.push(label);
        }
    }

    // Core = everything without a recognized prefix
    let core: Vec<&str> = BUILTIN_NAMES.iter().copied()
        .filter(|n| best_category(n).is_none())
        .collect();
    if !core.is_empty() {
        out.push_str(&format!("**core**: {}\n\n", backtick_join(&core)));
    }

    // Prefixed categories (in defined order)
    for label in &seen_labels {
        let names: Vec<&str> = BUILTIN_NAMES.iter().copied()
            .filter(|n| best_category(n) == Some(label))
            .collect();
        if !names.is_empty() {
            out.push_str(&format!("**{}**: {}\n\n", label, backtick_join(&names)));
        }
    }

    // -- UGens (from synthdef::UGEN_NAMES) ----------------------------------
    out.push_str("## UGens (inside define blocks)\n\n");
    out.push_str(&backtick_join(UGEN_NAMES));
    out.push_str("\n\n");

    // -- Default params (from synthdef::DEFAULT_PARAMS) ---------------------
    out.push_str("## Default SynthDef Params\n\n");
    let params_str: Vec<String> = DEFAULT_PARAMS
        .iter()
        .map(|(k, v)| format!("`{}`={}", k, v))
        .collect();
    out.push_str(&params_str.join(" "));
    out.push_str("\n\n");

    // -- Execution model ----------------------------------------------------
    out.push_str("## Execution\n\n");
    out.push_str(concat!(
        "- `audion run file.au` — runs file, auto-calls `main()` if defined, waits for threads\n",
        "- `audion run file.au --watch` — reloads on save, caches SynthDefs\n",
        "- `audion` — REPL mode (no auto main)\n",
        "- Ctrl+C frees all synth nodes cleanly\n",
    ));

    out
}
