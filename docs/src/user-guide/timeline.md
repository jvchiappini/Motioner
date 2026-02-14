# Timeline Editor

The timeline editor is the heart of Motioner, providing precise control over your animations.

## Timeline Basics

### Playhead

The **playhead** (red vertical line) indicates the current frame position.

- **Click** — Jump to frame
- **Drag** — Scrub through animation
- **Keyboard** — Use arrow keys for frame-by-frame navigation

### Time Ruler

The time ruler shows:
- Frame numbers
- Time markers
- Duration

### Frame Rate

Set your project's frame rate in the Properties panel:
- **24 fps** — Film standard
- **30 fps** — Video standard
- **60 fps** — Smooth motion
- **Custom** — Any value

## Working with Keyframes

### Adding Keyframes

1. Move playhead to desired frame
2. Adjust property value
3. Keyframe is auto-created (or manually add)

### Editing Keyframes

- **Select** — Click keyframe marker
- **Move** — Drag to new position
- **Delete** — Select and press `Delete`
- **Copy/Paste** — Standard shortcuts

### Keyframe Interpolation

_(Coming in future versions)_

- Linear
- Ease In/Out
- Bezier curves
- Step (no interpolation)

## Layers

### Layer System

Organize animation elements in layers:
- Stacking order (top = foreground)
- Individual visibility toggle
- Lock layers to prevent editing

### Layer Operations

- **Add Layer** — Right-click timeline > Add Layer
- **Rename** — Double-click layer name
- **Delete** — Select and press `Delete`
- **Reorder** — Drag layer up/down

## Timeline Navigation

### Shortcuts

| Action | Shortcut |
|--------|----------|
| Play/Pause | `Space` |
| Next Frame | `Right Arrow` |
| Previous Frame | `Left Arrow` |
| Jump to Start | `Home` |
| Jump to End | `End` |
| Zoom In | `+` or `Scroll Up` |
| Zoom Out | `-` or `Scroll Down` |

### Range Selection

1. Click and drag on time ruler
2. Selected range highlights
3. Operations apply to selection

## Best Practices

### Frame Accuracy

- Use frame stepping (`Left`/`Right`) for precision
- Zoom in for detailed keyframe placement
- Set frame rate before starting project

### Organization

- Name layers descriptively
- Group related elements
- Use color coding (future feature)

### Performance

- Limit active layers to visible elements
- Use layer locking to prevent accidental edits
- Preview in sections for complex animations

## Next Steps

- [Creating Animations](./creating-animations.md)
- [Exporting Projects](./export.md)
