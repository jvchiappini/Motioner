# Motioner UI target v1 (design reference for prompt 15)

Status: reference, 2026-11-04. Audience: whoever builds GUIMaker tokens,
components and the Motioner migration (prompt 15 phases 1, 6, 7, 10).
This is the visual contract. A screen is done when it matches the layout
sketches below and passes the acceptance checks at the end.

Source of truth today: `crates/motioner_ui/src/theme.rs` (palette, sizes),
`layout.rs` (shell split), `start.rs`, `panels/*`. Keep the palette; replace
the per-call numbers with the tokens below.

## 1. Principles

1. **The canvas is the hero.** Chrome is quiet: low contrast surfaces, one
   accent, no decoration that competes with the user's frame.
2. **Alignment by construction.** Every edge sits on the 4 px grid; text
   baselines inside a row align across panels (layers row 3 = timeline track 3).
3. **One accent, used for meaning only:** selection, primary action, playhead,
   focus ring. Never for decoration.
4. **Density like a pro tool** (After Effects, Figma, DaVinci): 28 px rows,
   12 px body text, but generous 12-16 px outer padding in panels.
5. **Every interactive thing has 5 states:** rest, hover, pressed, focused
   (keyboard), disabled. Selected is a sixth for list rows and tracks.
6. **Motion is short and functional:** 120 ms for hover/press, 180 ms for
   panel open/close, ease-out; nothing animates while scrubbing or playing.

## 2. Tokens

### 2.1 Neutrals (dark theme)

Keep the ramp but make it monotonic. Today `N6 = #2A2C33` is darker than
`N5 = #30323A`: fix it so `N6 = #3A3D46` (borders on raised surfaces).

| Token | Value | Use |
|---|---|---|
| `surface.pasteboard` | N0 #0A0B0D | area around the frame |
| `surface.app` | N1 #111215 | window background, gaps between panels |
| `surface.panel` | N2 #17181C | panel bodies |
| `surface.raised` | N3 #1E1F24 | panel headers, popovers, cards |
| `surface.control` | N4 #26282E | inputs, buttons at rest |
| `surface.control.hover` | N5 #30323A | |
| `border.hairline` | N4 at 100 % | 1 px dividers inside panels |
| `border.strong` | N6 #3A3D46 | input outlines, card outlines |
| `text.primary` | N10 #F0F2F5 | titles, values |
| `text.body` | N9 #C9CDD5 | labels |
| `text.muted` | N8 #9095A1 | captions, units, timestamps |
| `text.disabled` | N7 #5E626D | |

Contrast floor: `text.body` on `surface.panel` >= 7:1, `text.muted` >= 4.5:1.
`theme.rs` already has `contrast()`; make it a unit test over every pair
used in the token table.

### 2.2 Semantic colours

Accent #5B8CFF (hover #7AA2FF), danger #F2555A, warning #F5A524,
success #3FCF8E, keyframe #E8C25A. Soft variants = colour at 14 % alpha
over the surface (selection fill, danger banner background).

Track colours by layer kind (timeline bars and layer icons), desaturated so
they read on N2: shape #6C8CF5, text #B48CF0, image #4FB8A8, video #E08A5A,
audio #5FBF6A, group #8A90A0.

### 2.3 Spacing, radii, elevation, type

- Spacing scale: 2, 4, 8, 12, 16, 24, 32, 48. Nothing else.
- Radii: 4 (inputs, buttons, rows), 6 (cards, popovers), 8 (dialogs,
  start-screen tiles). Track bars: 3.
- Elevation: `e0` flat; `e1` popover/menu = raised + 1 px `border.strong`
  + shadow 0 4 16 #000 @ 40 %; `e2` dialog = shadow 0 12 40 #000 @ 55 % and a
  scrim #000 @ 50 % behind.
- Type (Inter or the engine default sans; tabular numerals for all numbers):

| Token | Size / line | Weight | Use |
|---|---|---|---|
| `caption` | 11 / 16 | 400 | units, timestamps, ruler |
| `body` | 12 / 16 | 400 | labels, rows |
| `body.strong` | 12 / 16 | 600 | panel headers, selected row |
| `emphasis` | 13 / 20 | 500 | input values, menu items |
| `title` | 15 / 20 | 600 | dialog titles |
| `display` | 22 / 28 | 600 | start screen heading only |

## 3. Shell (editor)

```
+----------------------------------------------------------------------------+
| [M] demo ▾  File Edit View   |   ◀ ▶ ▶▶  00:02:14 / 00:06:00  |  Export ▸ (●)|  40
+-----------+----------------------------------------------+-----------------+
| LAYERS  + |                                              | INSPECTOR       |  32
|-----------|                                              |-----------------|
| ▾ ▣ title |                                              | Transform    ▾  |
|   T sub   |              +--------------------+          |  Position x 960 |
| ▣ bg      |              |                    |          |           y 540 |
| ▶ clip    |              |      frame         |          |  Scale   100 %  |
| ♪ music   |              |   (pasteboard N0)  |          |  Rotation  0°   |
|           |              +--------------------+          | Fill         ▾  |
|-----------|                                              |  ■ #FFFFFF  100%|
| ASSETS    |                         [fit ▾] [100 %]      |                 |
| ░ ░ ░     |                                              |                 |
+-----------+----------------------------------------------+-----------------+
| ⏮ ◀ ▶ ▶ ⏭  ⟲   00:02:14 ▏          ruler 0s   1s   2s ▼   3s   4s   5s     |  44+24
| title  ◆  |██████████◆█████████████◆                                       |  28
| sub       |    ███████████████████                                         |
| clip      |▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒                                 |
| music     |∿∿∿∿∿∿∿∿∿∿∿∿∿∿∿∿∿∿∿∿∿∿∿∿∿∿∿∿∿∿∿∿∿∿∿∿∿∿∿∿∿∿∿∿∿                       |
+----------------------------------------------------------------------------+
| ● Saved 14:19 · 1920×1080 · 60 fps · 61 fps UI                   GPU 3 %   |  24
+----------------------------------------------------------------------------+
```

Rules:
- Panel gap: 1 px of `surface.app` between panels (no borders drawn twice).
- Panel header 32 px: `body.strong` uppercase-free title (sentence case),
  actions right-aligned as 24 px icon buttons, 8 px apart.
- Top bar: transport and timecode centred on the window, not on the canvas.
  Timecode uses tabular numerals so it never jitters.
- Canvas: the frame has a 1 px `border.hairline` and `e1` shadow; zoom controls
  float bottom-right on a raised pill, 8 px from the edges.
- Timeline: track names column width = left panel width, so names line up
  under the layers list. Playhead = accent 1 px line + 10 px triangle head on
  the ruler. Keyframes = 8 px diamonds in `keyframe`, selected ones get a
  1 px white outline. Waveforms for audio, filmstrip thumbnails for video at
  zoom levels where a frame is >= 48 px wide.
- Status bar: `caption`, muted; save state dot (success / warning while dirty).

## 4. Inspector

```
| Transform                      ▾ |   section header 28, body.strong
|  Position   [x  960 ] [y  540 ]  |   label column 88, fields share the rest
|  Scale      [  100 %         ]◆  |   ◆ = keyframe toggle, 16 px, right edge
|  Rotation   [    0°          ]◆  |
|  Opacity    [====●------] 100 %  |   slider + numeric
|----------------------------------|   hairline between sections
| Fill                           ▾ |
|  ■ #FFFFFF                100 %  |   swatch 16, hex emphasis, alpha muted
```

- Numeric fields: drag on the label to scrub, click to type, Enter commits,
  Esc reverts, Tab moves to the next field. Units in `text.muted` inside the
  field, right-aligned.
- Animated property: keyframe toggle filled; value text in accent when the
  playhead is on a keyframe.
- Value came from code vs. from UI (D-0027 origins): small 4 px dot left of
  the label (muted = code, accent = edited in UI). No other colouring.

## 5. Layers and assets

- Layer row 28 px: 16 indent per level, disclosure 12, kind icon 16 in the
  track colour, name `body`, eye and lock icons appear on hover only (always
  visible when toggled off).
- Selected row: accent at 14 % fill + 2 px accent bar on the left edge.
- Assets: grid of 72 px tiles (thumbnail, 2-line name `caption`, duration
  badge for media); list view toggle in the header. Drag ghost = tile at 70 %.

## 6. Start screen

```
+--------------------------------------------------------------+
|  [M] Motioner                                         ⚙      |
|                                                              |
|   Good evening                       display, 48 from top    |
|                                                              |
|   +----------+ +----------+ +----------+                     |
|   |  + New   | |  Open…   | | Examples |   3 action tiles    |
|   | project  | |          | |          |   160 x 96, r8      |
|   +----------+ +----------+ +----------+                     |
|                                                              |
|   Recent                                         View all    |
|   +--------+ +--------+ +--------+ +--------+                |
|   | thumb  | | thumb  | | thumb  | | thumb  |  16:9, r6      |
|   | demo   | | promo  | | intro  | | test   |                |
|   | 2 h ago| | Mon    | | Oct 28 | | Oct 20 |                |
|   +--------+ +--------+ +--------+ +--------+                |
+--------------------------------------------------------------+
```

- Content column max width 960, centred; columns of the recent grid are
  responsive (min tile 200 px).
- The primary tile ("New project") is the only accent-filled element.
- Empty state (no recents): one line of muted text and the example project
  card instead of an empty grid.

## 7. Dialogs and menus

- Dialog: width 480 (forms) or 640 (pickers), radius 8, `e2`, 24 padding,
  title `title`, footer buttons right-aligned, primary rightmost, Esc closes,
  Enter triggers primary.
- Menu: `e1`, radius 6, 4 padding, rows 28, shortcut in `text.muted`
  right-aligned, separators hairline with 4 px vertical margin.
- Tooltip: 600 ms delay, raised, `caption`, 6 x 4 padding, never over the
  pointer.

## 8. Acceptance checks (screenshot loop, prompt 15 phase 5)

Capture at 1280x720, 1600x900 and 2560x1440 (scale 1, 1.5, 2) for: start
screen (with and without recents), editor with `examples/demo.motioner`
loaded, inspector with a selected text layer, an open menu, a dialog.

1. No overlaps reported by the GUIMaker diagnostics.
2. Every rect edge is on the 4 px grid at scale 1.
3. Layer rows and timeline tracks share the same y for the same layer.
4. No literal colours, radii or sizes outside the token module
   (grep check in the gate).
5. Contrast test from §2.1 passes.
6. Idle editor: 0 redraws per second with nothing changing.
7. Side-by-side before/after screenshots in the phase report.
