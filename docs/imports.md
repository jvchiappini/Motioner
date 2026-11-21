# Imports: `use()` and definitions

`use(path)` is the one import surface (D-0051). It returns a **definition**
whose kind comes from the file extension. A definition is a value with
metadata, and calling it instantiates it in the document.

```python
art = use("assets/mascot.svg")          # symbol: callable, .parts
logo = use("assets/logo.png")           # image definition
sfx = use("audio/pop.wav")              # audio definition
rigs = use("rigs/mascot.motion")        # module namespace

s = art(pos=(960, 540), scale=3)        # a group with addressable parts
n = logo(key=None)                      # same node as image("assets/logo.png")
m = rigs.build(s)                       # module functions build into this document
```

| Extension | Definition | Metadata |
|---|---|---|
| `.svg` | symbol | `kind` (implicit), `width`, `height`, `name`, `parts` |
| `.motion` | module namespace | top-level names are attributes |
| `.png .jpg .jpeg .webp` | image | `kind`, `path` |
| `.wav .flac .mp3 .ogg` | audio | `kind`, `path` |
| `.mp4 .mov .mkv` | video | `kind`, `path` |
| `.ttf .otf` | font | `kind`, `path` |

`image("key")`, `audio(...)` and `video(...)` keep working and produce
byte-identical documents; `use()` is the general form, not a replacement
with different semantics.

## Resolution and sandbox

- A resolver is injected at the `run_*` entry point. `motioner_io` exposes
  `ProjectAssets::new(root)`; the CLI render, still, info and desktop paths
  use it. Tests use an in-memory resolver.
- Image, audio, video and font definitions never touch the resolver: their
  path is stored on the node and resolved by the asset store, exactly like
  `image("key")` today.
- SVG and `.motion` reads are sandboxed to the root: absolute paths and
  parent escapes outside the root are rejected **before** any I/O.
- A relative path inside a module resolves against that module's directory
  (`rigs/mascot.motion` may `use("../assets/mascot.svg")` when that stays
  inside the root).

## Modules

A `.motion` module executes once per run in the shared global scope and
returns a namespace of its top-level definitions:

```python
# rigs/mascot.motion
art = use("../assets/mascot.svg")

def build(pos=(0, 0), scale=1.0):
    m = rig(art, pos=pos, scale=scale)
    return m
```

```python
rigs = use("rigs/mascot.motion")
m = rigs.build(pos=(960, 540), scale=4.2)
```

- Imports are idempotent per resolved path; a cycle is a typed diagnostic.
- Module diagnostics name the module path and line and point at the `use()`
  call site.
- v0 limitation: names are imported into one global scope (like a guarded
  `from module import *`), so two modules exporting the same name shadow
  each other. Namespaced globals are backlogged.

## Hot reload

Re-running the program rebuilds the document. Nodes created by an import
keep their provenance identity, so selection, overrides and keys that
address parts by id survive an SVG edit; a removed id is reported as an
orphan by the editor's resolver work (T-RIG-0014).
