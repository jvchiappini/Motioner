# Claude session start — review a finished phase of opencode prompt 10

Speak Spanish with me. Write code, docs and commits in English.

## Role

Architect and **reviewer** of opencode's work on
`.control/prompts/opencode/opencode-prompt-10-professional-motion.md`
(professional motion: 12 phases, opencode stops after EACH phase so I can
compact). I paste opencode's phase report at the end of this message.

Reading, building, running tests and taking screenshots into `out/` is
allowed. Do not change code or docs unless it is a small fix needed to land
the phase (say exactly what you changed); bigger issues go into the next
phase's prompt for opencode.

## State to expect

- Phases 1-3 are committed and pushed on `feat/m1-foundations`
  (D-0028 model, D-0029 paths, D-0030 gradients). Last pin bump:
  FerrousEngine `857b9df` (Pure2D gradients); `motioner_render` has
  `engine-gradients` on by default (prompt for phase 4 asked opencode to
  remove the feature).
- opencode never commits. It may leave changes in the FerrousEngine clone
  `C:\Users\jvchi\CARPETAS\FerrousEngine` (branch
  `feat/pure2d-text-and-readback`); Motioner must use the submodule
  `Motioner\engine\ferrous`, never the clone.

## Folders

- Motioner: `C:\Users\jvchi\CARPETAS\Motioner`.
- FerrousEngine clone: `C:\Users\jvchi\CARPETAS\FerrousEngine`.
- Prompts: `Motioner\.control\prompts\{claude,opencode}\`.

## Steps

1. `PYTHONIOENCODING=utf-8 python .control/scripts/pctl.py status`;
   `git status` / `git log --oneline -5` in Motioner, the submodule and the
   engine clone; free space on C: (stop and ask below 5 GB).
2. Check the report against the tree: the files it names exist, the
   goldens' SHA-256 match, nothing touched `engine/ferrous` directly.
3. If the engine clone has changes: read the diff, commit them in the clone
   (conventional message), push, move the submodule to the new head
   (`git -C engine/ferrous fetch --depth 2 origin feat/pure2d-text-and-readback`
   then `checkout FETCH_HEAD`) and make Motioner build and pass against it
   (features behind `Unsupported` must now really render; goldens must run,
   not skip).
4. Gate with `CARGO_INCREMENTAL=0` and `-j 2`: `cargo fmt -- --check`,
   `cargo clippy --workspace --all-targets -j 2 -- -D warnings`,
   `cargo test --workspace -j 2 --no-fail-fast`,
   `RUSTDOCFLAGS=-D warnings cargo doc --workspace --no-deps -j 2`,
   `python scripts/check_boundaries.py`, `cargo machete crates`,
   `pctl validate`. `hello_f120.png` must stay byte-identical.
5. Quick review: D-0024 (every new action is a `Command` with parity tests
   and docs), no duplication, typed errors, no panics; look at the phase's
   screenshots in `out/ui/`.
   **FerrousEngine is a general-purpose engine, not a Motioner backend.**
   Review engine changes as public engine features: generic API with no
   Motioner/MDF concepts, rustdoc, crate docs in `crates/<crate>/docs` and
   the `mkdocs.yml` nav, engine tests (and ideally a small example), engine
   `cargo clippy`/`cargo test` for the touched crates green, no breaking
   change without a migration note. Flag Motioner-only workarounds that
   belong in the engine.
6. Tell me in a few lines: what the phase did, the gate result, problems
   ordered by severity (file:line). Then **ask me** before committing.
7. When I say so: commit and push by layer (code+docs+examples+goldens,
   then `.control`, then the pin bump as its own commit), ending every
   message with `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`.
8. Give me, ready to paste, the short message for opencode's NEXT phase:
   only that phase, render capabilities built in FerrousEngine as general
   engine features (API, docs, tests, example) that Motioner consumes,
   stop at the phase boundary with the gate green, no
   commit/stage/push, fix list from my review first, disk rule (clean
   `target/debug/incremental` below 6 GB, stop below 5 GB), report in
   Spanish with golden SHAs, engine clone state and free disk.

## Gotchas

- Bash heredocs containing `'''` break in this harness: write scripts to the
  scratchpad with the Write tool.
- Windows PowerShell 5.1 drops unescaped `"` in native arguments.
- The submodule is shallow: fetch the branch, not a bare SHA.
- 8 GB RAM: always `-j 2`.

## opencode's report

(paste here)
