# Brief — Data Story

**Prompt.** "A six-second corporate data story: one question, then quarterly
growth as a bar chart with a counting headline. Calm, legible, one idea per
beat. 16:9."

**Assumptions.** Four quarters of invented but plausible revenue; English
text; corporate theme; silent.

**Design rationale.** One idea per scene. The question gets its own beat so
the audience reads it before the chart appears; the counter and the bars then
share the second scene, with the counter above the chart so the number is the
first thing read. The chart enters once (group rise) rather than animating
every bar, which keeps the motion quiet.

`count_up` is a sequence of text layers switched with `hold` keys (MDF text
content is static); sixteen steps read as a smooth count without a new
document channel.

**Plan.**

| Time | Scene | Beats |
|---|---|---|
| 0.0-2.4 s | `question` | headline reveal, supporting line fade |
| 2.4-6.0 s | `chart` | counter counts to $84M, chart rises with labels |

**Render.** `motioner render examples/data_story.motion --out out/data_story.mp4`
