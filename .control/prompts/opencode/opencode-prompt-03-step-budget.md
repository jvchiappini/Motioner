# opencode prompt 03 — deterministic step budget for the MDF interpreter

You are working in the Motioner repository at `C:\Users\jvchi\CARPETAS\Motioner`
(Rust 1.96.0, edition 2021, branch `feat/m1-foundations`). A hardening pass on
`motioner_dsl` was just reviewed. It is mostly approved, but the review found
one major defect. Fix exactly that defect. Work alone; do not use subagents
(every file below is touched by the same change).

## The defect

The sandbox bounds time only by counting statements
(`crates/motioner_dsl/src/interpreter.rs:141-150`). Operations inside one
statement can do unbounded work:

1. Exponential work on shared values. This 3-line program makes `str()` and
   `==` take time `2^n` while every list has 2 items and depth is only `n`:
   - `a = [0]` / `for i in range(n):` / `    a = [a, a]` then `s = str(a)`.
   - Same with a second independent list `b` built the same way, then `x = a == b`.
   Measured in release: n=22 takes 1.8 s for `str`; n=40 would take hours or
   exhaust memory before `max_string_len` is checked (the string is built
   first and checked afterwards at `crates/motioner_dsl/src/builtins.rs:28-32`).
2. Linear work multiplied by the statement count. A loop of 10,000,000
   statements, each doing `x = l == l` or `s = str(l)` on a 1,000,000-item
   list, performs ~10^13 item visits. `Value::equals` walks the whole value
   in `check_depth` before the identity short-circuit
   (`crates/motioner_dsl/src/value.rs:254-300`).

Spec section 12 (`spec/mdf-v0.md:205-217`) promises a bounded run; the
current implementation does not deliver it.

## Goal

Replace the statement counter with one deterministic **step budget** that
every statement and every bulk operation charges, so the total work of any
program is bounded by `Limits::max_steps` regardless of value shapes.

## Design (follow it; ask nothing, report deviations)

1. `crates/motioner_dsl/src/limits.rs`
   - Rename `Limits::max_statements` to `max_steps` (default stays
     10,000,000). Rustdoc: one step per executed statement, plus one step per
     collection item visited, copied or materialized, plus one step per
     `STRING_BYTES_PER_STEP` bytes of string produced. Update the struct
     rustdoc that currently says time is bounded by `max_statements`.
   - Add `pub const STRING_BYTES_PER_STEP: usize = 64;` (single definition).
   - Add a `pub(crate) struct Budget` that owns the remaining steps and has
     `pub(crate) fn charge(&mut self, steps: u64) -> HostResult<()>` (saturating
     arithmetic, never panics) and a helper to charge string bytes
     (rounding up). The error is a limit error with the message
     `execution step limit of {max_steps} exceeded`.
   - `Budget` is created from `&Limits`; it is the only place that knows how
     steps are counted.
2. `crates/motioner_dsl/src/interpreter.rs`
   - Replace the `statements: u64` field and its check with a `Budget`.
     Each executed statement charges 1 step, as today.
   - Pass `&mut Budget` to every operation that loops over items: list,
     tuple and string concatenation (charge the combined length/bytes before
     allocating, after the existing limit checks), cycle checks
     (`contains_shared_list`, one step per container visited), equality,
     and builtin calls.
3. `crates/motioner_dsl/src/value.rs`
   - `equals`, its depth check, `display` and `contains_shared_list` charge
     one step per container item visited. Keep `max_value_depth` checks and
     their existing messages.
   - Rewrite `display` to append into a single `String` buffer (no
     intermediate `Vec<String>` per container) and fail with the existing
     `string length limit of N bytes exceeded` message as soon as the buffer
     passes `max_string_len`, instead of after building the whole string.
     Reuse `Limits::check_string_len`; do not add a second copy of that check.
   - Keep the identity short-circuit for `==` only if the depth check can no
     longer cost more than the budget allows; correctness first, the
     existing test `deep_values_fail_display_and_equality_with_a_limit` must
     still pass unchanged.
4. `crates/motioner_dsl/src/builtins.rs`
   - `call` receives `&mut Budget`. `items()` charges the number of items it
     materializes (strings: characters; ranges: length) before allocating.
     `sorted`, `reversed`, `enumerate`, `zip`, `min`, `max`, `str` charge per
     item processed. `str` charges string bytes via the helper.
5. `spec/mdf-v0.md` section 12 only: replace the "Maximum statements
   executed" bullet with the step definition above, and add bullets for the
   existing caps that are missing (`max_list_len` 1,000,000 items,
   `max_value_depth` 100, expression depth `MAX_EXPR_DEPTH` 4096). Do not
   change any other section.

## Tests (add to `crates/motioner_dsl/tests/value_safety.rs` unless noted)

- `shared_values_cannot_blow_up_display`: the `a = [a, a]` program with
  n = 64 fails with `ErrorCategory::Limit` (step or string limit) and the
  test asserts the run finishes in under 2 s in debug.
- `shared_values_cannot_blow_up_equality`: two independent `[a, a]` chains
  with n = 64 compared with `==` fail with a limit error in under 2 s.
- `repeated_bulk_operations_hit_the_step_limit`: with
  `Limits { max_steps: 100_000, ..Default }`, a loop of 1,000 iterations
  doing `x = l == l` on a 1,000-item list fails with the step-limit message.
- `step_limit_counts_concatenation_and_iteration`: small budget, show that
  `a + b` on lists and `for c in "..."` over a long string are charged.
- In `crates/motioner_dsl/tests/execute.rs:255` change only the needle
  `"statement limit"` to `"step limit"` and the struct literal field name
  (`max_statements` → `max_steps`); same in `tests/fuzz.rs`. No other edit to
  existing tests; no test may be weakened or deleted.
- Existing suites must still pass with default limits, including
  `million_deep_list_builds_and_drops_iteratively`,
  `million_deep_tuple_builds_and_drops_iteratively` and
  `tests/examples.rs` (`examples/hello.motion`).
- Prove at least the two blow-up tests fail without the fix: describe how
  (for example, temporarily set the per-item charge to zero) and report the
  observed failure. Restore the code afterwards.

## Out of scope

- No changes to `motioner_core`, `motioner_eval`, the parser, `scene.rs`,
  `host.rs` (except a signature forced by `&mut Budget`, report it),
  any `Cargo.toml` or `Cargo.lock` (three are intentionally dirty: do not
  touch them), anything under `engine/` or `.control/`.
- No wall-clock timers, no new dependencies, no `unsafe`, no crate-level
  `#[allow]`.
- No memory accounting beyond the existing caps.

## Rules

- Rustdoc on every public and `pub(crate)` item you add or change.
- Typed errors through the existing `HostError`; never panic on input.
- Do not commit, stage, stash, reset or checkout. Never run `cargo fmt --all`.

## Verification (run with `CARGO_INCREMENTAL=0`, all must exit 0)

- `cargo fmt -- --check`
- `cargo clippy --workspace --all-targets -- -D warnings`
- `cargo test --workspace`
- `cargo test --workspace --release`
- `RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps`
- `python scripts/check_boundaries.py`
- `cargo machete crates` (exit 1 is expected, only for `motioner_render`)

Stop and report instead of continuing if: a check fails twice after a fix
attempt, the design forces edits outside the allowed files, or free space on
C: drops below 8 GB.

## Final report

1. Files changed, one line each.
2. Public API changes (exact signatures).
3. Each test added, and the evidence that the blow-up tests fail without the fix.
4. Default-limit headroom: steps used by `examples/hello.motion` and by the
   million-deep list test (print them once from a temporary test, then remove it).
5. Verification table: command and exit code.
6. Deviations and open questions.
