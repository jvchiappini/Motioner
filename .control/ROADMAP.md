# Roadmap

Top-level entry point to the Motioner roadmap. Phases live in
`roadmaps/phases/`, initiatives in `roadmaps/initiatives/`, and
milestones in `roadmaps/milestones/`. The index at `roadmaps/_index.md`
is generated.

Strategy: ship a deterministic 2D vertical slice first, then add 3D and
compositing, then the code and AI layers that define the product. Every
phase keeps preview and export on the same evaluator.

---

## Phases

| Phase | Status | Focus | File |
|---|---|---|---|
| PHASE-0001 | in_progress | Foundations and contracts | [details](roadmaps/phases/PHASE-0001.md) |
| PHASE-0002 | not_started | Core document and animation engine | [details](roadmaps/phases/PHASE-0002.md) |
| PHASE-0003 | not_started | Editor UI and 2D motion graphics | [details](roadmaps/phases/PHASE-0003.md) |
| PHASE-0004 | not_started | 3D, effects and compositing | [details](roadmaps/phases/PHASE-0004.md) |
| PHASE-0005 | not_started | Code, AI and production pipeline | [details](roadmaps/phases/PHASE-0005.md) |

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
| Code and document diverge | 2, 5 | High | High | Document is truth; code is a lossless projection with round-trip tests |
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

---

## Resolved decisions

- D-0001: FerrousEngine is the rendering foundation.
- D-0002: deterministic frame-based evaluation.
- D-0003: the document is the truth; code is a lossless projection.
- D-0004: engine-native Ferrous UI plus GUIMaker; no egui.
- D-0005: native Motioner Document Format (MDF); no RON/JSON.
- D-0006: document and runtime are separate; compiled render plans.
- D-0007: all mutations through typed commands.
- D-0008: unified 2D/3D layer ordering.
- D-0009: sandboxed deterministic script runtime.

## Remaining setup decisions

- Engine and GUIMaker pinning mechanism (submodule vs fork branch).
- GUIMaker revision and widget contribution boundary.
- Script runtime implementation for D-0009.
- GitHub repository replacement and remote layout.

## How to use

1. Create a phase: `python .control/scripts/pctl.py roadmap-phase-new "Name" --orden N`
2. Create an initiative: `python .control/scripts/pctl.py roadmap-initiative-new "Name" PHASE-XXXX`
3. Create a milestone: `python .control/scripts/pctl.py roadmap-milestone-new "Name" INITIATIVE-XXXX PHASE-XXXX`
4. Change state: `python .control/scripts/pctl.py roadmap-touch <ID> --estado in_progress`
5. Rebuild the index: `python .control/scripts/pctl.py reindex`
