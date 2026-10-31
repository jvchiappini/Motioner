//! Fuzz-style robustness tests: malformed input must produce diagnostics,
//! never panics or hangs.
//!
//! The generator is a fixed-seed xorshift so failures are reproducible
//! without an external fuzzing tool.

use motioner_dsl::{execute, parse, Limits};

/// Deterministic xorshift64* generator.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_f491_4f6c_dd1d)
    }

    fn below(&mut self, bound: usize) -> usize {
        usize::try_from(self.next() % bound as u64).unwrap_or(0)
    }
}

const FRAGMENTS: &[&str] = &[
    "def ",
    "f",
    "(",
    ")",
    ":",
    "\n",
    "    ",
    "if ",
    "elif ",
    "else",
    "for ",
    " in ",
    "range(3)",
    "return ",
    "break",
    "continue",
    "pass",
    "x",
    " = ",
    " += ",
    "1",
    "2.5",
    "\"s\"",
    "'t'",
    "[",
    "]",
    ",",
    ".",
    "pos",
    "key",
    "(1, 2)",
    " + ",
    " - ",
    " * ",
    " / ",
    " // ",
    " % ",
    " == ",
    " < ",
    " and ",
    " or ",
    "not ",
    "True",
    "None",
    "rect(10, 10)",
    "circle(5)",
    "text(\"a\")",
    "comp(name=\"c\")",
    "group()",
    "#c",
    "\t",
    "{",
    "}",
    "while ",
    "use(",
    "\\",
    "\"",
    "@",
    "é",
    "999999999999999999999",
    "1e400",
    "-",
    "\r\n",
];

fn small_limits() -> Limits {
    Limits {
        max_steps: 5_000,
        max_call_depth: 32,
        max_nodes: 100,
        max_string_len: 4_096,
        max_list_len: 256,
        max_value_depth: 100,
    }
}

/// Parses and, when that succeeds, executes; any outcome but a panic is fine.
fn exercise(source: &str) {
    if let Ok(program) = parse(source) {
        let _ = execute(&program, &small_limits());
    }
}

#[test]
fn random_fragment_programs_never_panic() {
    let mut rng = Rng(0x9e37_79b9_7f4a_7c15);
    for _ in 0..3_000 {
        let count = 1 + rng.below(40);
        let source: String = (0..count)
            .map(|_| FRAGMENTS[rng.below(FRAGMENTS.len())])
            .collect();
        exercise(&source);
    }
}

#[test]
fn mutated_example_never_panics() {
    let example = include_str!("../../../examples/hello.motion");
    let chars: Vec<char> = example.chars().collect();
    let mut rng = Rng(0x1234_5678_9abc_def1);
    for _ in 0..1_000 {
        let mut mutated = chars.clone();
        for _ in 0..=rng.below(4) {
            let at = rng.below(mutated.len() + 1);
            match rng.below(3) {
                0 if at < mutated.len() => {
                    mutated.remove(at);
                }
                1 => {
                    let fragment = FRAGMENTS[rng.below(FRAGMENTS.len())];
                    for (offset, c) in fragment.chars().enumerate() {
                        mutated.insert(at + offset, c);
                    }
                }
                _ if at < mutated.len() => {
                    let swap = rng.below(mutated.len());
                    mutated.swap(at, swap);
                }
                _ => {}
            }
        }
        exercise(&mutated.into_iter().collect::<String>());
    }
}

#[test]
fn random_bytes_never_panic() {
    let mut rng = Rng(42);
    for _ in 0..2_000 {
        let bytes: Vec<u8> = (0..rng.below(64))
            .map(|_| (rng.next() & 0x7f) as u8)
            .collect();
        exercise(&String::from_utf8_lossy(&bytes));
    }
}

/// Names that a hostile program can feed arithmetic results into. They
/// cover every implemented builtin, every builtin named by the spec (an
/// unimplemented name still has to fail cleanly) and the scene
/// constructors.
const BUILTIN_NAMES: &[&str] = &[
    "len",
    "range",
    "str",
    "int",
    "float",
    "bool",
    "min",
    "max",
    "abs",
    "round",
    "sorted",
    "reversed",
    "enumerate",
    "zip",
    "seed",
    "rand",
    "sin",
    "cos",
    "tan",
    "radians",
    "degrees",
    "lerp",
    "clamp",
    "comp",
    "group",
    "rect",
    "circle",
    "text",
];

/// Hostile values written only with literals and arithmetic: NaN, +inf,
/// -inf, a huge finite number and a tiny subnormal.
const HOSTILE_VALUES: &[&str] = &[
    "(1e308 * 1e308) - (1e308 * 1e308)",
    "1e308 * 1e308",
    "-(1e308 * 1e308)",
    "1e308",
    "1e-308",
    "0 - (1e308 * 1e308)",
];

fn flat_expression(terms: usize) -> String {
    let mut source = String::with_capacity(4 * terms);
    source.push('1');
    for _ in 1..terms {
        source.push_str(" + 1");
    }
    source
}

#[test]
fn seeded_hostile_values_cycles_and_long_expressions_never_panic() {
    let mut programs: Vec<String> = Vec::new();

    // Deep acyclic values: display and equality must stop at the depth
    // limit instead of recursing, and the values must drop iteratively.
    programs.push("l = [0]\nfor i in range(300):\n    l = [l]\ns = str(l)\n".to_owned());
    programs.push("t = (0,)\nfor i in range(300):\n    t = (t,)\ns = str(t)\n".to_owned());
    programs.push(
        "a = [0]\nb = [0]\nfor i in range(300):\n    a = [a]\n    b = [b]\nx = a == b\n".to_owned(),
    );

    // Cyclic assignments through lists, tuples and augmented assignment.
    for program in [
        "a = [0]\na[0] = a\n",
        "a = [0]\nb = (a,)\na[0] = b\n",
        "a = [0]\nb = [a]\na[0] = b\n",
        "a = [[0]]\na[0] += [a]\n",
    ] {
        programs.push(program.to_owned());
    }

    // Long flat expressions: past MAX_EXPR_DEPTH they are rejected by the
    // parser before a huge tree can exist.
    programs.push(format!("x = {}\n", flat_expression(5_000)));
    programs.push(format!("x = {} and 1\n", flat_expression(5_000)));

    // Deterministic seeded combinations of NaN/inf arithmetic and every
    // builtin name. A fixed table plus a seeded generator keeps failures
    // reproducible while covering argument shapes that the table misses.
    let mut rng = Rng(0x6a09_e667_f3bc_c909);
    for _ in 0..200 {
        let name = BUILTIN_NAMES[rng.below(BUILTIN_NAMES.len())];
        let value = HOSTILE_VALUES[rng.below(HOSTILE_VALUES.len())];
        programs.push(format!("x = {name}({value})\n"));
    }
    for name in BUILTIN_NAMES {
        for value in HOSTILE_VALUES {
            programs.push(format!("x = {name}({value}, {value})\n"));
        }
    }

    for program in &programs {
        exercise(program);
    }
}
