# Frontend handoff — known bugs before the GUIMaker migration

This page is the handoff for the separate Claude frontend session. Phase 10 of
the GUIMaker prompt does the plumbing only; it does not rewrite the frontend.
These defects are visible in the current start screen and are fixed by that
session, not by the library phases.

## Known bugs

- **Clicking a form field closes the dialog.** `StartScreen::action_at` returns
  `StartAction::Cancel` for a click inside a field (`start.rs:368-371`). A
  field click should focus the field, never cancel.
- **The New project card is transparent.** The card draws no surface behind
  its content, so the start screen's background shows through the dialog.
- **Labels overlap the fields.** In the New project dialog the label column and
  the field column are computed independently and share the same y range; the
  labels sit on top of the fields at 1280×720.
- **The Create button is clipped.** The bottom action row extends past the
  card's rounded corner and is cut by the card clip at 1280×720.

Reference captures: `out/ui/p15-00-before-start.png` (1600×900 in-app) and
`out/ui/p15-00-before-newproject.png` (1280×720 real screen, cropped).

Phase 10 does not touch this geometry. The Claude frontend session rebuilds
the start screen and the New project dialog on the GUIMaker view tree, which
replaces the hand-computed `StartLayout`, so these bugs go away with the old
layout code instead of being patched in place.
