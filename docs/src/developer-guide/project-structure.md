# Project Structure

Understanding Motioner's file organization.

## Directory Layout

```
Motioner/
├── .github/              # GitHub-specific files
│   ├── workflows/        # CI/CD workflows
│   ├── ISSUE_TEMPLATE/   # Issue templates
│   └── pull_request_template.md
├── assets/               # Application assets
│   ├── icons/           # UI icons
│   ├── fonts/           # Custom fonts
│   └── presets/         # Animation presets
├── docs/                # Documentation (mdBook)
│   ├── book.toml        # mdBook configuration
│   └── src/             # Documentation source
├── src/                 # Source code
│   ├── animations/      # Animation implementations
│   ├── main.rs          # Entry point
│   ├── app_state.rs     # Application state
│   ├── ui.rs            # User interface
│   ├── scene.rs         # Scene management
│   ├── timeline.rs      # Timeline editor
│   ├── canvas.rs        # Drawing canvas
│   ├── renderer.rs      # Rendering engine
│   ├── project_settings.rs
│   ├── welcome_modal.rs
│   ├── code_panel/
│   │   ├── autocomplete.rs   # moved inside editor submodule
│   ├── code_panel.rs
│   ├── dsl.rs
│   └── composition.wgsl # GPU shaders
├── target/              # Build artifacts (gitignored)
├── Cargo.toml           # Project manifest
├── Cargo.lock           # Dependency lock file
├── rust-toolchain.toml  # Rust version
├── LICENSE              # Apache 2.0 license
└── README.md            # Project overview
```

## Source Files

### Core Application

| File | Purpose |
|------|---------|
| `main.rs` | Application entry point, eframe setup |
| `app_state.rs` | Central state management, coordinates components |
| `ui.rs` | All UI rendering and layout |

### Animation System

| File | Purpose |
|------|---------|
| `scene.rs` | Scene structure and management |
| `scene_graph.rs` | Scene hierarchy (if present) |
| `timeline.rs` | Timeline editor logic |
| `animations/` | Animation type implementations |

### Rendering

| File | Purpose |
|------|---------|
| `renderer.rs` | Frame rendering and export |
| `canvas.rs` | Canvas drawing and interaction |
| `composition.wgsl` | WGSL shaders for GPU rendering |

### Utilities

| File | Purpose |
|------|---------|
| `project_settings.rs` | Project configuration |
| `welcome_modal.rs` | Welcome screen UI |
| `code_panel/autocomplete.rs` | Autocomplete functionality (part of editor module) |
| `code_panel.rs` | Code editor panel |
| `dsl.rs` | Domain-specific language |

## Configuration Files

### Cargo.toml
```toml
[package]
name = "motioner_ui"
version = "0.1.0"
edition = "2021"

[dependencies]
eframe = { version = "0.26", features = ["wgpu"] }
egui = "0.26"
# ... other dependencies
```

### rust-toolchain.toml
Specifies Rust version for consistent builds across environments.

## Build Artifacts

### target/
Contains all build outputs:
- `debug/` — Debug builds
- `release/` — Optimized builds
- `doc/` — Generated documentation

**Note:** This directory is gitignored and can be safely deleted.

## Assets Organization

### Current Structure
```
assets/
└── (to be populated with project assets)
```

### Recommended Structure
```
assets/
├── icons/
│   ├── app_icon.png
│   └── ui_icons/
├── fonts/
│   └── custom_font.ttf
├── presets/
│   └── animation_presets.json
└── examples/
    └── sample_project.motioner
```

## Documentation Structure

### docs/
```
docs/
├── book.toml            # mdBook config
├── src/
│   ├── SUMMARY.md       # Table of contents
│   ├── introduction.md
│   ├── user-guide/
│   ├── developer-guide/
│   ├── advanced/
│   ├── examples/
│   └── reference/
└── book/               # Generated output (gitignored)
```

## Next Steps

- [API Reference](./api-reference.md)
- [Contributing](./contributing.md)
