# Brief — Product Teaser

**Prompt.** "A six-second teaser for a noise-cancelling headphone called
Aurora. Neon, premium, a new badge, the product name and a short benefit
line, then a pre-order button. 16:9."

**Assumptions.** No product photo was supplied, so the title card carries the
product name and the glow-orbs background supplies the premium light. English
text, neon theme, silent.

**Design rationale.** Neon needs strong contrast, so all text uses the theme
`text`/`muted` tokens over the dark background. The badge drops in first to
signal novelty, the title card pops after it, and the second scene resets the
composition: logo lockup above, one button below, nothing else competing.

**Plan.**

| Time | Scene | Beats |
|---|---|---|
| 0.0-3.4 s | `hero` | badge drop, title card pop |
| 3.4-6.0 s | `cta` | logo lockup rise, pre-order button rise |

**Render.** `motioner render examples/product_teaser.motion --out out/product_teaser.mp4`
