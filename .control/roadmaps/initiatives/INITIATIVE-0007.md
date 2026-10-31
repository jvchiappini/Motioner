---
id: INITIATIVE-0007
nombre: "Editor shell and panels"
phase: PHASE-0003
estado: backlog
prioridad: alta
tamano_estimado: large
depende_de: ["INITIATIVE-0006"]
creado: 2026-10-31
responsable: agente
---

# Initiative: Editor shell and panels

## Description

Build the application shell of the editor: window layout, docking, panels,
menus, command palette, shortcuts and layout persistence. The shell
provides the frame in which the viewport, timeline, inspector, project
browser, code editor and chat live, without owning business logic.

## Scope

### Deliverables
- Docking workspace with resizable, floatable panels.
- Project/Assets panel, Inspector, Timeline panel, Viewport panel,
  Render queue and Code/Chat placeholders.
- Menu bar, context menus and a searchable command palette.
- Central command dispatch shared with keyboard shortcuts.
- Layout persistence and reset.
- Theme tokens and a widget spike for the riskiest controls.

### Non-goals
- Viewport rendering and timeline interaction internals.
- AI chat implementation.
- Installer and packaging.

## Technical approach

Panels are thin views over the document and selection state; edits are
emitted as commands. A dock manager owns geometry and persistence. The
toolkit decision (D-0004) must be resolved before panel work starts; a
spike validates docking, virtualized lists, text editing and graph
widgets early.

Key considerations:
- Keep redraws scoped to dirty panels.
- Avoid a god-app struct; use explicit state modules per panel.
- Make every action reachable through the command palette.

## Dependencies

- INITIATIVE-0006: stable core and render harness.
- D-0004 approved.

## Milestones

| Milestone | Description | Target date | Status |
|---|---|---|---|
| M-0004 | Interactive editor preview at target fps | | backlog |

## Tasks

No tasks yet. Create them when this initiative starts.

## Acceptance criteria

- [ ] The editor opens a project and restores the previous layout.
- [ ] Core actions are reachable without a mouse.
- [ ] Panels contain no business logic and emit commands.
- [ ] The UI stays responsive while rendering.

## Notes

The panel skeleton should exist before polish. A working ugly shell
early is worth more than a beautiful panel late.
