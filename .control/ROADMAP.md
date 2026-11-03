# Roadmap

Top-level entry point to the Motioner roadmap. Phases live in
`roadmaps/phases/`, initiatives in `roadmaps/initiatives/`, and
milestones in `roadmaps/milestones/`. The index at `roadmaps/_index.md`
is generated.

Strategy: ship a deterministic 2D vertical slice first, then add 3D and
compositing, then the code and AI layers that define the product. Every
phase keeps preview and export on the same evaluator.

## 1.0 scope (confirmed 2026-10-31)

Windows-only and 1080p-first:

- 2D motion graphics, AE-style timeline, masks, mattes, precomps.
- NLE: cut, trim, arrange, transitions and sync for video and audio.
- Audio: stereo mixer with volume, pan, fades and basic effects.
- 3D: glTF, cameras, lights, PBR, skeletal animation, GPU particles.
- Node graph as an alternate compositing view.
- Python-like executable MDF code with loops and control flow.
- AI chat with preview rendering, visual critique and automatic
  iteration; human approval for the final render.
- MP4 and PNG export; the user downloads and uploads to YouTube manually.

Post-1.0: 1440p/4K, rigid-body physics, cloth/fluids, color grading,
advanced audio mastering, HDR, macOS/Linux, plugin API, collaboration,
cloud rendering.

---

## Phases

| Phase | Status | Focus | File |
|---|---|---|---|
| PHASE-0001 | in_progress | Foundations and contracts | [details](roadmaps/phases/PHASE-0001.md) |
| PHASE-0002 | not_started | Core document and animation engine | [details](roadmaps/phases/PHASE-0002.md) |
| PHASE-0003 | not_started | Editor, NLE and audio | [details](roadmaps/phases/PHASE-0003.md) |
| PHASE-0004 | not_started | 3D, node graph and simulation | [details](roadmaps/phases/PHASE-0004.md) |
| PHASE-0005 | not_started | Code, AI and 1.0 delivery | [details](roadmaps/phases/PHASE-0005.md) |

### Phase 1: Foundations and contracts

Freeze reversible-expensive decisions, scaffold the workspace, pin
FerrousEngine and prove deterministic still-frame rendering.

### Phase 2: Core document and animation engine

Document model, commands and undo, timeline, evaluator and headless
render harness. No UI.

### Phase 3: Editor UI and 2D motion graphics

After Effects-style shell, viewport, timeline, 2D layers, typography,
preview and MP4 export.

### Phase 4: 3D, effects and compositing

3D layers, cameras, lights, PBR, render graph, effects, color
management, masks, mattes and precomps.

### Phase 5: Code, AI and production pipeline

Bidirectional motion language, AI agent with typed tools, caching,
render queue and performance hardening.

---

## Cross-phase risks

| Risk | Affected phases | Likelihood | Impact | Mitigation |
|---|---|---|---|---|
| FerrousEngine API churn | 1-5 | High | High | Pin a revision; isolate with an adapter crate; upstream small patches |
| GUIMaker lacks editor widgets | 1, 3 | High | High | Gap analysis and spike on docking, rows and bezier graph first |
| Code and document diverge | 2, 5 | High | High | Program-first MDF with provenance, overrides and round-trip tests |
| Determinism breaks on real GPUs | 1-4 | Medium | High | Golden frames, CPU oracle, documented tolerance |
| Scope creep into a full NLE or game engine | 2-5 | Medium | High | Non-goals in GOALS.md; phase gates |
| Real-time target missed | 3-5 | Medium | High | Budgets from Phase 1, proxies, caches, benchmarks |

## Key milestones (cross-phase)

| Milestone | Phase | Description | File |
|---|---|---|---|
| M-0001 | 1 | Architecture decisions approved | [details](roadmaps/milestones/M-0001.md) |
| M-0002 | 1 | First deterministic frame from a project file | [details](roadmaps/milestones/M-0002.md) |
| M-0003 | 2 | Ten-second 2D composition animates headlessly | [details](roadmaps/milestones/M-0003.md) |
| M-0004 | 3 | Interactive editor preview at target fps | [details](roadmaps/milestones/M-0004.md) |
| M-0005 | 3 | MP4 export matches preview | [details](roadmaps/milestones/M-0005.md) |
| M-0006 | 4 | 3D layer composited over 2D with effects | [details](roadmaps/milestones/M-0006.md) |
| M-0007 | 5 | AI creates a composition from a prompt | [details](roadmaps/milestones/M-0007.md) |
| M-0008 | 5 | Thirty-second 1080p render with caching | [details](roadmaps/milestones/M-0008.md) |
| M-0009 | 5 | 1.0 release candidate | [details](roadmaps/milestones/M-0009.md) |

---

## Resolved decisions

- D-0001: FerrousEngine is the rendering foundation.
- D-0002: deterministic frame-based evaluation.
- D-0003 superseded by D-0011: program-first executable MDF.
- D-0004: engine-native Ferrous UI plus GUIMaker; no egui.
- D-0005: native Motioner Document Format (MDF); no RON/JSON.
- D-0006: document and runtime are separate; compiled render plans.
- D-0007: all mutations through typed commands.
- D-0008: unified 2D/3D layer ordering.
- D-0009: sandboxed deterministic script runtime.
- D-0010: confirmed 1.0 scope (Windows, 1080p, NLE, audio, node graph, particles, AI iteration).
- D-0011: program-first Python-like MDF; custom DSL; provenance and overrides.
- D-0012: AI providers, cloud data, budgets and human approval.
- D-0013: Apache-2.0 license and permissive asset policy.
- D-0014: 1080p-first reference performance.
- D-0015: revenue from donations/sponsors, paid cloud rendering and AI credits.
- D-0016: LaTeX math typesetting in 1.0.
- D-0017: 1.0 delivery defaults.
- D-0036: UI edits write MDF code (overrides as fallback); edit traces feed a small motion-design code model; in-app agent harness on the same command surface.
- D-0039: motion standard library (themes, relative layout, beats, presets, components) before any training corpus.
- D-0040: training data, quality rubric, vision data and evaluation targets for the Motioner model.
- D-0041: agent harness workflow, clarifying questions, vision and providers (incl. Motioner API and local model).
- D-0042: improvement loop from user signals with opt-in consent and a release gate.
- D-0043: agent tool catalogue (signatures, results, images, budgets).

## Remaining implementation details

- Engine and GUIMaker pinning mechanism and update process.
- GUIMaker revision and widget contribution boundary.
- MDF interpreter implementation and performance budget.
- Override and bake semantics; orphan reporting.
- LaTeX renderer choice and supported command set.
- Installer technology (MSI versus NSIS).
- Audio effect parameter ranges and presets.
- Cloud pricing and cost display.
- Shared module (`use`) semantics.

## How to use

1. Create a phase: `python .control/scripts/pctl.py roadmap-phase-new "Name" --orden N`
2. Create an initiative: `python .control/scripts/pctl.py roadmap-initiative-new "Name" PHASE-XXXX`
3. Create a milestone: `python .control/scripts/pctl.py roadmap-milestone-new "Name" INITIATIVE-XXXX PHASE-XXXX`
4. Change state: `python .control/scripts/pctl.py roadmap-touch <ID> --estado in_progress`
5. Rebuild the index: `python .control/scripts/pctl.py reindex`
