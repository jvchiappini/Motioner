---
id: INITIATIVE-0020
nombre: "AI automatic iteration and critique loop"
phase: PHASE-0005
estado: backlog
prioridad: critica
tamano_estimado: xlarge
depende_de: ["INITIATIVE-0014"]
creado: 2026-10-31
responsable: agente
---

# Initiative: AI automatic iteration and critique loop

## Description

Make the AI not only generate a first draft but render it, inspect the
frames and improve it until the result meets the request. This is the
product's core differentiator and needs strict cost, time and approval
controls.

## Scope

### Deliverables
- Plan-act-review loop: understand the request, edit, render a preview,
  inspect frames, critique and refine.
- Vision input from rendered frames and contact sheets.
- Iteration budgets: maximum iterations, time and cost.
- Change review, undo grouping and provenance per iteration.
- Human approval gates for final render and expensive operations.
- Reproducible logs of prompts, tools and results.

### Non-goals
- Autonomous publishing or uploads.
- Training custom models.
- Unbounded background agents.

## Technical approach

The loop is an orchestrator over the same typed tools as the chat. Each
iteration produces a document transaction and a preview render. A
vision-capable model critiques specific frames against the request and
the loop decides whether to continue. Budgets stop the loop safely.

Key considerations:
- Render cost per iteration; previews use proxy settings.
- Token and cost accounting per task.
- Deterministic tool execution and recorded fixtures for tests.
- User can pause, cancel or take over at any time.

## Dependencies

- INITIATIVE-0014: AI agent and tool protocol.
- INITIATIVE-0008: viewport playback and preview.
- INITIATIVE-0009: export pipeline.

## Milestones

| Milestone | Description | Target date | Status |
|---|---|---|---|
| M-0007 | AI creates a composition from a prompt | | backlog |

## Tasks

No tasks yet. Create them when this initiative starts.

## Acceptance criteria

- [ ] A prompt produces a first draft, previews and at least one
      self-critique iteration.
- [ ] Iterations stop at the configured budget and report the cost.
- [ ] Every iteration is reviewable, undoable and visible as code.
- [ ] The final render requires explicit human approval.

## Notes

The loop must be inspectable. A black box that spends money silently is
not acceptable.
