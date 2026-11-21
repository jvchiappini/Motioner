# Brief — Kinetic Title

**Prompt.** "A bold editorial title card for a design studio reel: a small
kicker, a giant headline, an accent rule, then a short two-line tagline.
Calm, confident, no gimmicks. 16:9, about six seconds."

**Assumptions.** English on-screen text, editorial theme, tempo 120, music
not supplied (silent example).

**Design rationale.** Editorial pairs a warm paper background with a tight
type scale; the accent rule gives the eye an anchor between the headline and
the tagline. Motion is staggered: kicker fades, the headline rises, the rule
wipes, then the tagline lines follow 0.25 s apart so the reading order is
never ambiguous.

**Plan.**

| Time | Scene | Beats |
|---|---|---|
| 0.0-3.6 s | `title` | kicker fade, headline reveal (rise), accent rule wipe |
| 3.6-6.0 s | `tagline` | line A rise, line B rise 0.25 s later |

**Render.** `motioner render examples/kinetic_title.motion --out out/kinetic_title.mp4`
