---
id: PHASE-0005
nombre: "Code, AI and 1.0 delivery"
orden: 5
estado: not_started
inicio: 2026-10-31
fin_estimado: ""
fin_real: null
depende_de: ["PHASE-0004"]
responsable: agente
prioridad: alta
---

# Phase: Code, AI and production pipeline

## Summary

Deliver the product differentiators: a readable, bidirectional motion
language; an AI agent that can generate and edit compositions through a
typed tool protocol; and the performance work that makes long
compositions, caching and background rendering practical.

The phase ends when an AI prompt produces a complete animated
composition that a user can inspect as code, edit, preview and render to
a 30-second 1080p video with caching, and 1.0 ships as a signed Windows
release.

## Goals

- Make the whole document expressible as deterministic, versionable
  code without creating a second source of truth.
- Let the AI perform safe, undoable, previewable edits through tools.
- Meet the real-time and export performance budgets on the reference
  machine.

## Scope

### In scope
- Python-like Motioner Language with loops, functions and control flow.
- Parser, printer, validation, formatting and sandboxed execution.
- AI provider abstraction, chat panel, tool schema and change review.
- Automatic render-critique-iterate loop with cost and time budgets.
- Frame cache, proxies, background render queue, progress and recovery.
- Windows installer, portable build, licensing and release automation.
- Benchmarks, profiling, memory and VRAM budgets.

### Out of scope
- Team collaboration and cloud rendering.
- A public plugin API and marketplace.
- Training or fine-tuning custom models.

## Dependencies

- PHASE-0004: full 2D/3D compositing feature set.
- AI provider credentials and network access.

## Risks and mitigations

| Risk | Likelihood | Impact | Mitigation |
|---|---|---|---|
| Code and document diverge | High | High | Round-trip tests; one writer path; code is a projection of the document |
| AI produces destructive edits | High | High | Typed commands, previews, undo and approval gates |
| Prompt context exceeds limits | Medium | Medium | Compact project summaries and selective retrieval, PCS-style |
| Cache invalidation bugs | Medium | High | Revision hashes on inputs; conservative invalidate; cache tests |

## Key decisions

- D-0011: Python-like executable MDF with control flow (proposed).
- D-0012: AI providers, cloud data and human approval.
- D-0013: Apache-2.0 license and asset policy.
- D-0015: revenue model (proposed).
- D-0007: AI edits only through commands and tools.
- D-0009: sandboxed deterministic script runtime.

## Success criteria

- [ ] Any composition round-trips document -> code -> document without
      semantic loss.
- [ ] An AI prompt creates a multi-layer animated comp and iterates on
      rendered previews within budget, with human approval for the final
      render.
- [ ] Python-like code with loops and conditionals round-trips with the
      document.
- [ ] A 30-second 1080p render completes with caching and a progress
      report on the reference machine.
- [ ] A signed Windows installer and portable ZIP are published under
      Apache-2.0.
- [ ] Real-time preview meets 1080p 60 fps at full quality on the
      reference machine.

## Initiatives

- INITIATIVE-0013 - Bidirectional motion language
- INITIATIVE-0014 - AI agent and tool protocol
- INITIATIVE-0015 - Caching, render queue and performance
- INITIATIVE-0020 - AI automatic iteration and critique loop
- INITIATIVE-0021 - 1.0 packaging, distribution and monetization

## Notes

This phase is where Motioner becomes the product described in the
vision. Earlier phases must not absorb its scope implicitly.
