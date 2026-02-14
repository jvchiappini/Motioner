# Interface Overview

Motioner's interface is designed for efficiency and ease of use. This page provides an overview of the main UI components.

## Main Window Layout

```
┌─────────────────────────────────────────────────┐
│  Menu Bar                                       │
├─────────────────────────────────────────────────┤
│                                                 │
│              Canvas / Preview Area              │
│                                                 │
├─────────────────────────────────────────────────┤
│              Timeline Panel                     │
├─────────────────────────────────────────────────┤
│  Properties │         Inspector                 │
└─────────────────────────────────────────────────┘
```

## Components

### Menu Bar

The top menu bar provides access to:

- **File** — Project operations (New, Open, Save, Export)
- **Edit** — Undo, Redo, Preferences
- **View** — Toggle panels, zoom controls
- **Help** — Documentation, About

### Canvas / Preview Area

The main canvas displays:

- Real-time animation preview
- Current frame visualization
- Interactive scene elements
- Grid and guides (optional)

**Shortcuts:**
- `Space` — Play/Pause preview
- `Left/Right` — Navigate frames
- `Scroll` — Zoom in/out

### Timeline Panel

The timeline shows:

- Frame markers and playhead
- Animation keyframes
- Layer structure
- Duration settings

**Features:**
- Drag playhead to scrub timeline
- Click to add keyframes
- Right-click for context menu

### Properties Panel

Configure:

- Scene settings (FPS, dimensions)
- Export options
- Animation properties
- Layer parameters

### Inspector

View and edit:

- Selected element properties
- Keyframe values
- Easing curves
- Metadata

## Customization

### Theme

Motioner uses the egui default theme. Future versions will include:
- Light/Dark mode toggle
- Custom color schemes
- Font size adjustment

### Panel Layout

- Drag panel edges to resize
- Toggle panel visibility from View menu
- Reset layout: `View > Reset Layout`

## Next Steps

- [Timeline Editor](./timeline.md) — Master the timeline
- [Creating Animations](./creating-animations.md) — Build animations
