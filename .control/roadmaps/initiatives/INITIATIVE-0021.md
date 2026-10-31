---
id: INITIATIVE-0021
nombre: "1.0 packaging, distribution and monetization"
phase: PHASE-0005
estado: backlog
prioridad: alta
tamano_estimado: medium
depende_de: ["INITIATIVE-0020"]
creado: 2026-10-31
responsable: agente
---

# Initiative: 1.0 packaging, distribution and monetization

## Description

Turn the feature set into a professional Windows release: installers,
signed binaries, release automation, license and notices, documentation
and the first revenue path compatible with Apache-2.0.

## Scope

### Deliverables
- Windows installer and portable ZIP.
- Code signing and reproducible release builds.
- GitHub release automation with changelog and checksums.
- Apache-2.0 LICENSE, NOTICE, third-party notices and asset licenses.
- Documentation site and onboarding tutorial.
- Crash reporting with explicit opt-in and no project data.
- Revenue path: donations/sponsors and optional paid services.

### Non-goals
- Auto-update in 1.0 (decide later).
- macOS and Linux installers.
- Paid proprietary features in the open core.

## Technical approach

Releases are built from tags in CI without a GPU. Installers package the
binary, assets and licenses. Versioning follows semantic versioning.
Monetization starts with donations and sponsorships; paid cloud
rendering and AI credits are separate services, not feature locks.

Key considerations:
- Legal review of bundled assets and dependencies.
- No secrets in the binary or repository.
- Clear system requirements and GPU notes.
- A stable update and support policy.

## Dependencies

- All 1.0 feature initiatives.
- D-0013 license and D-0015 revenue model.

## Milestones

| Milestone | Description | Target date | Status |
|---|---|---|---|
| M-0009 | 1.0 release candidate | | backlog |

## Tasks

No tasks yet. Create them when this initiative starts.

## Acceptance criteria

- [ ] A signed Windows installer and portable ZIP are published.
- [ ] The release includes license, notices and third-party attribution.
- [ ] Documentation covers install, first project and export.
- [ ] A donation/sponsorship channel exists and is linked.
- [ ] No proprietary paywall blocks core features.

## Notes

1.0 is the credibility milestone. Quality of packaging and legal hygiene
matters as much as features.
