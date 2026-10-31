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
        max_statements: 5_000,
        max_call_depth: 32,
        max_nodes: 100,
        max_string_len: 4_096,
        max_list_len: 256,
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
