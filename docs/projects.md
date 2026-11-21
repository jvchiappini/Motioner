# Projects

A Motioner project is a folder named `<name>.motioner/`. Everything the
editor creates lives inside it, so a project can be copied, backed up or
moved as one directory. The contract is D-0045; the language surface is in
`spec/mdf-v0.md`.

## Layout

```
<name>.motioner/
  project.motion          manifest: version, settings, asset registry,
                          composition list and the main composition
  compositions/<slug>.motion
  compositions/<slug>.overrides.motion   only while overrides exist
  assets/<sha256>.<ext>   immutable content-addressed blobs
  cache/                  decoded media, peaks, proxies, thumbnails
  backups/                rolling snapshots
  traces/                 reserved for write-through traces (D-0036)
  editor.motion           editor state: layout, locked/expanded rows,
                          last frame. Never rendered.
```

`project.motion`, `compositions/`, `assets/` and `editor.motion` are
authoritative. `cache/`, `backups/` and `traces/` are runtime data and may be
deleted at any time; the project still opens and rebuilds them. The folder is
recognised by the presence of `project.motion`.

## Manifest and asset registry

`project.motion` is a plain MDF program of constant calls, one per line, so
the same parser and diagnostics serve project files:

- `project(version, name, width, height, fps, duration, background, main)`
  appears once. `version` is the format version; `main` names a composition
  slug.
- `asset(key, hash, original, kind, imported)` appears zero or more times,
  sorted by key. `kind` is `image`, `font`, `audio` or `video`; `hash` is the
  SHA-256 of the bytes; `original` is the imported file name; `imported` is
  UTC seconds.
- `composition(slug, title)` appears at least once, sorted by slug.

Unknown calls, unknown arguments and duplicate keys are rejected with the
file and line. Saving a loaded project twice is byte-identical.

The key starts as the original file stem, is case-insensitively unique and
gets a numeric suffix on a clash (`logo`, `logo-2`, ...). MDF names the key,
not the path, so renaming the file in Explorer does not break the project.

## MDF resolution

`image("logo.png")`, `audio("music.wav")` and `video("clip.mp4")` resolve
through the registry by key **or** original name, project assets first. A
bare name that matches no registry entry is still tried as a project-relative
path (for `assets/fonts/...` and loose files). Absolute paths and `..` are
rejected. The registry key is the explicit form: `image("logo")` selects the
`logo` entry even when its original file is `logo.png`.

The project root replaces the process working directory for fonts, images,
audio and video. A loose `.motion` file keeps single-file mode: the root is
the process working directory, there is no registry and no autosave.

## Backups and recovery

Every committed edit writes the affected files atomically (temp file in the
same directory, then rename). A debounced overrides write happens 300 ms after
the last change; a full snapshot is written to `backups/` every two minutes
and before migrations. The snapshot policy keeps a bounded count and size and
prunes the oldest.

While a project is open, `cache/session.lock` holds the PID and a heartbeat.
If the app starts and the lock is stale while the newest backup is newer than
the files on disk, the status bar offers **Restore** and **Dismiss**. Restore
copies the backup over the composition files; Dismiss keeps what is on disk
and removes the offer.

## App-level state

Recents, window placement and preferences live in
`%APPDATA%\Motioner\app.motion` (MDF constants). Recent entries are capped at
12 and are kept when the folder is missing, shown greyed with "remove" rather
than silently deleted.

## Safe to delete

- `cache/` — decoded PCM, peaks, proxies, thumbnails and the session lock.
  The next open rebuilds what it needs; a proxy is regenerated on first play.
- `backups/` — snapshots. Delete them if the project is known good.
- `traces/` — reserved and currently empty.

Deleting `assets/`, `compositions/`, `project.motion` or `editor.motion`
loses data. If an asset blob is missing, `motioner info` and the app report a
typed error naming it; MDF sources are never silently dropped.

## Command line

```
motioner new     <dir> [--name N] [--size WxH] [--fps N] [--duration FRAMES]
motioner migrate <file.motion> <dir> [--name N]
motioner import  <project> <file>...
motioner info    <project>
motioner backup  <project>
motioner render  <project> --out out.mp4 [--no-audio]
motioner still   <project> --frame N --out out.png
motioner preview <project>
```

Every command accepts the folder, its `project.motion` or one of its
composition files. `motioner` with no arguments opens the start screen:
New project, Open project, recent projects and the examples.

## Demo project

`examples/demo.motioner/` is a complete project: a generated background
image, a generated 60 fps video clip and a generated music loop, with a title
sequence and a fade-out. Every asset comes from ffmpeg `lavfi` test sources
(`gradients`, `testsrc2`, `aevalsrc`), so the example has no third-party
media. Render it with:

```
motioner render examples/demo.motioner --out out/demo.mp4
```

The demo goldens live in `crates/motioner_testkit/golden/demo_f090.png` and
`demo_f330.png`; `motioner_render/tests/golden_demo.rs` pins them and the
exact video source frame, and `motioner_io/tests/demo_mixer.rs` pins the
first 4096 mixed samples.
