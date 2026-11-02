# FerrousEngine submodule

`engine/ferrous` is a git submodule for FerrousEngine, the wgpu-based
engine Motioner renders with, pinned rather than vendored. Source:
https://github.com/jvchiappini/FerrousEngine.

Pinned commit: `06a8adf` (2026-11-02, typography: block layout, GPOS,
per-glyph draw;
branch `feat/pure2d-text-and-readback`, see D-0023).

Only `motioner_render` consumes the engine path dependencies
(`ferrous_2d`, `ferrous_font`, `ferrous_renderer`). The root
`Cargo.toml:14` excludes `engine/`, so workspace commands never enter the
engine's own workspace.

## Why shallow and sparse

The checkout is shallow (depth 1) and sparse to keep clone time and disk
usage low. The excluded directories hold large assets that Motioner does
not need to build, test or render.

Sparse patterns, non-cone mode, relative to the engine repository root:

```
/*
!/assets/skybox/
!/assets/models/
!/brain/
!/crates/ferrous_web/models/
!/crates/ferrous_ui_render/test_batch
```

## Fresh clone

```
git submodule update --init --depth 1
cd engine/ferrous
git sparse-checkout set --no-cone "/*" "!/assets/skybox/" "!/assets/models/" "!/brain/" "!/crates/ferrous_web/models/" "!/crates/ferrous_ui_render/test_batch"
```

In Git Bash, prefix the command with `MSYS_NO_PATHCONV=1`; otherwise MSYS
rewrites the leading-slash patterns.

## Changing the engine

Never treat the working copy here as the source of truth. Engine changes
are made on a branch in the FerrousEngine repository as small,
upstreamable patches. Once a patch lands, bump the pin and record an ADR.

## Bumping the pin

1. Fetch and check out the target commit:

```
cd engine/ferrous
git fetch --depth 1 origin <target-commit>
git checkout <target-commit>
```

2. Build and run Motioner checks from the repository root:

```
$env:CARGO_INCREMENTAL=0; cargo build --workspace
$env:CARGO_INCREMENTAL=0; cargo test --workspace
$env:CARGO_INCREMENTAL=0; cargo clippy --workspace --all-targets -- -D warnings
python scripts/check_boundaries.py
```

3. Commit the new gitlink from the Motioner root; the parent repo records
   the submodule SHA.
4. Record an ADR with `pctl.py decision-new` (old and new commit, date,
   reason, checks), then run `pctl.py reindex`.
