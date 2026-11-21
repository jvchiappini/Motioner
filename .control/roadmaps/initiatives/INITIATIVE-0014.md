---
id: INITIATIVE-0014
nombre: "AI agent and tool protocol"
phase: PHASE-0005
estado: backlog
prioridad: critica
tamano_estimado: xlarge
depende_de: ["INITIATIVE-0013"]
creado: 2026-10-31
responsable: agente
---

# Initiative: AI agent and tool protocol

## Description

Make chatting with an AI a first-class way to author videos. The agent
reads a compact project summary, calls typed tools that mutate the
document through commands, previews its changes and explains them. Every
AI action is undoable, reviewable and attributable.

## Scope

### Deliverables
- Provider abstraction with streaming and tool calling.
- Chat panel with conversation, attachments and rendered previews.
- Tool schema covering project inspection, layer creation, property
  animation, effects, asset import and render preview.
- Compact context builder with selection, outline and relevant docs.
- Change review: diff, accept/reject, undo grouping.
- Safety: no shell, no arbitrary file access, confirmation for
  destructive or expensive actions.
- Real providers only (D-0041 amendment): offline tests for request
  building, stream parsing of captured payloads and tool execution; E2E
  tests against the real API when a key is present.

### Non-goals
- Model training or fine-tuning.
- Autonomous long-running agents without user checkpoints.
- Cloud project storage.

## Technical approach

Tools map one-to-one to document commands and return typed results with
enough context for the model to continue. The context builder sends a
summary, not the whole document, and retrieves details on demand. All
mutations are grouped into a reviewable transaction with a human-facing
summary.

Key considerations:
- Token budgets and caching of stable context.
- Deterministic seeds and recorded fixtures for tests.
- Clear provenance: which tool and prompt produced a change.
- Fail closed when a tool precondition is not met.

## Dependencies

- INITIATIVE-0013: code representation and sandbox.
- D-0007 approved.
- AI provider API keys.

## Milestones

| Milestone | Description | Target date | Status |
|---|---|---|---|
| M-0007 | AI creates a composition from a prompt | | backlog |

## Tasks

No tasks yet. Create them when this initiative starts.

## Acceptance criteria

- [ ] A prompt creates a multi-layer animated composition.
- [ ] Every AI edit is undoable and reviewable as a diff.
- [ ] The agent cannot perform I/O or run shell commands.
- [ ] Offline tests cover tool flows; E2E sessions run against the real API.

## Notes

The AI is a client of the same command surface as the UI. If it needs a
private backdoor to be useful, the command model is wrong.

D-0036 (2026-11-03): the agent panel is a harness (conversation, provider
abstraction, tool calls) whose tools come from `motioner tools --schema`
(prompt 12). Its edits go through write-through, so they are MDF code and
are recorded as traces. Model training stays out of the app, but it is a
product goal: a small model distilled from a Fable 5 corpus plus traces.
