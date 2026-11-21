# Domain: motioner_analysis

## Purpose

Pure design and media analysis (D-0049). It reads a compiled document, an
evaluated render plan or decoded samples and returns measurements. No GPU, no
files, no editor.

## Key components

- `frame_elements` — projects plan items into elements with boxes, kind,
  text and paint order, and adds groups as the union of their descendants:
  `crates/motioner_analysis/src/element.rs`.
- `describe_frame`, `hit_test`, `sample_property` — the text twins of a
  render: `crates/motioner_analysis/src/describe.rs`,
  `crates/motioner_analysis/src/hit.rs`,
  `crates/motioner_analysis/src/sample.rs`.
- `motion_report_range` — onsets, duration, peak speed, velocity jumps at
  keys, eases, stagger, dead time and overload from sampled boxes:
  `crates/motioner_analysis/src/motion.rs`.
- `layout_report` — text/text and text/busy overlaps, safe area, off-screen,
  alignment groups and type levels: `crates/motioner_analysis/src/layout.rs`.
- `legibility_report` and `contrast_ratio` — reading time and WCAG contrast
  against a background sampler: `crates/motioner_analysis/src/legibility.rs`.
- `Rules` / `RULES` — the single threshold table behind `check_design`:
  `crates/motioner_analysis/src/rules.rs`.
- `check_design` — aggregates every report into one issue list:
  `crates/motioner_analysis/src/check.rs`.
- Media: `analyze_image`, `analyze_video`, `analyze_audio`:
  `crates/motioner_analysis/src/media.rs`.
- `code_for_node` / `nodes_for_code` over `motioner_dsl::ValueOrigins`:
  `crates/motioner_analysis/src/code.rs`.

## Boundaries

Depends only on `motioner_core`, `motioner_eval` and `motioner_dsl` (value
origins). Text measurement and background sampling are traits implemented by
the renderer and the tools runtime, never by this crate.

## Tests

Unit tests in `geom`, `text` and `legibility`; integration tests
`design.rs`, `presets.rs`, `fixtures.rs` and `media.rs` under
`crates/motioner_analysis/tests/`.
