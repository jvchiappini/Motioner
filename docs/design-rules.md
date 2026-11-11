# Design rules (`check_design`)

`check_design` implements the D-0040 §5 rubric as one rule table. The
thresholds live in `crates/motioner_analysis/src/rules.rs` (`Rules`); this
page documents them and points at the defect fixtures.

Every rule has a violating program and the fixed one under
`examples/design_defects/<rule>/` (`bad.motion`, `good.motion`). The test
`crates/motioner_analysis/tests/fixtures.rs` asserts that the bad program
fires the rule and the good one does not; render either with
`motioner still <file> --frame 30 --out <png>`.

`check_design` returns issues with the rule id, severity, node ids, frames,
the measured value, the threshold and a fix. Errors mean the frame is broken;
warnings mean the motion or layout reads wrong; infos are suggestions.

| Rule | Severity | Threshold (defaults) | Measures |
|---|---|---|---|
| `blank_frame` | error | no visible element on a sampled frame inside the composition | evaluated scene |
| `off_screen` | error | text, or a box at least 0.5 % of the canvas, entirely outside | evaluated boxes |
| `safe_area` | warning | inside the inner 90 % (title safe) | text boxes |
| `text_overlap` | error | overlap > 5 % of the smaller text box | text/text pairs |
| `text_on_busy` | warning | text over an image or gradient by > 15 % of the text box | scene + paint |
| `contrast` | error | >= 4.5:1 (>= 3:1 at 24 px or 18.66 px bold), sampled from a render without the text | WCAG 2.1 |
| `reading_time` | warning | >= 0.5 s + 0.3 s/word of visible time | evaluated visibility |
| `velocity_jump` | warning | <= 10 px/frame change of speed at a key whose ease is not `hold` | keyframe curves |
| `peak_speed` | warning | <= 64 px/frame | sampled box motion |
| `linear_ease` | info | a channel with 2-5 keys all `linear` | keyframe eases |
| `stagger` | info | three or more elements must not share one onset frame by less than 2 frames | onsets |
| `dead_time` | info | <= 2.0 s without movement | sampled box motion |
| `overload` | warning | <= 6 large movers (> 6 % of the canvas, > 4 px/frame) at once | sampled box motion |
| `type_hierarchy` | info | <= 5 distinct sizes; three or more texts at one size is not a hierarchy | text sizes |

## Good and bad

| Rule | Bad | Fixed |
|---|---|---|
| `blank_frame` | `examples/design_defects/blank_frame/bad.motion` | `good.motion` (background plate) |
| `contrast` | white text on white | near-black text on white |
| `dead_time` | one fade, then seven still seconds | a slow rotation keeps the frame alive |
| `linear_ease` | a 90° rotation with `linear` | `quad_in_out` |
| `off_screen` | text centred at (-400, -300) | text at the canvas centre |
| `overload` | nine large bars moving fast at once | two slower bars |
| `peak_speed` | 760 px in three frames | 760 px in 1.2 s with `quad_out` |
| `reading_time` | six words visible for 0.6 s | the same line for four seconds |
| `safe_area` | text at (4, 40) | text at (100, 120) |
| `stagger` | three titles fade in on the same frame | onsets 0.1 s apart |
| `text_on_busy` | white text over a black-to-white gradient | text on a solid surface |
| `text_overlap` | two 56 px lines 10 px apart | lines 120 px apart |
| `type_hierarchy` | three 40 px lines | 72/40/26 px |
| `velocity_jump` | a linear move then a 4x faster one | one `quad_out` move |

## Contrast

Contrast is measured against **what is really behind the text**, sampled
from a render with every text node hidden (`motioner_render::tools::TextlessFrame`).
A text node that the renderer could not place has no contrast sample and the
rule stays quiet rather than guessing.

## Applying the rules

- `motioner tool <project> check_design --args '{}'` runs the whole range.
- `motioner tool ... --args '{"start":0,"end":120,"rules":["contrast"]}'`
  filters by rule.
- The library must pass: every preset and component is checked on its own in
  `crates/motioner_analysis/tests/presets.rs` (three themes) and in the
  release matrix example (ten themes).
