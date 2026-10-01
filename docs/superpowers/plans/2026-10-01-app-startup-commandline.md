# App startup menu and visible command line

User reported the running Rust app has no menu and no command line (screenshot 2026-10-01). Prioritize this app interface correction before resuming bounded DIM/HATCH recovery. Local integration is already authorized; no remote actions.

## Global Constraints

- One source writer and one QEMU guest across recovery work. Finish DIM current case safely, preserve partial evidence, release leases before this app task; resume same DIM evidence worker afterward.
- Startup opens the requested drawing (existing default/path behavior) and loads the shipped ACAD menu through the existing file loader. Missing menu data must leave a usable command interface with a clear status; do not panic.
- Render an always visible command area at the bottom using the existing embedded bitmap font, displaying current prompt/input and the latest status/error. Show typing before Return. This is a Rust interface improvement, not a native pixel-fidelity claim.
- Drawing/menu paint, coordinate mapping, and mouse routing must agree on space reserved for the command area. Clicks in the command area cannot submit points or select entities. Preserve existing panel consumption, NEXT/GO/control semantics, and shared keyboard/GO Return path.
- Fit menu NEXT and the command area within the minimum physical client size; avoid tiny/zero-size panics. Keep menu unloading functional while the command area remains visible.
- No command semantics, DIM/HATCH geometry, oracle code, dependencies, OS clipboard changes, command-history/scrollback system, or unrelated refactoring.
- Allowed tracked writes: acad-app main.rs, a focused command_line.rs module if needed, focused app tests, README launch instructions, handover app-startup status. Preserve current test assertions unless the viewport/layout requirement necessarily updates their coordinates.
- Graph-first code discovery; scoped fallback when graph absent/stale. Independent task review after self-reviewed commit; one final whole-branch review and at most one combined fix wave/scoped rereview. No worker helpers.

## Task 1: Implement and verify startup menu and command area

- [ ] Load ACAD on normal startup through the existing menu loader, retaining missing-file diagnostics and explicit MENU unload/reload.
- [ ] Paint readable prompt/input and latest status/error at bottom, with clipping for long input and small windows. Reuse menu_panel draw_text; do not copy the font or add a UI framework. Keep the input end/caret observable when it exceeds width.
- [ ] Use one consistent available drawing/menu height for painting and mouse conversion. Consume command-area clicks before panel/drawing dispatch and clip crosshair/axis ticks outside the reserved area. Keep the existing command submission/callback paths intact.
- [ ] Update physical minimum size to accommodate the full panel and command area. Add meaningful focused tests for startup menu success/missing file, painted prompt/input/status, bottom-click consumption in point/selection modes, minimum sizing and unload state; reuse existing route suite for regression.
- [ ] Update README launch/menu/input instructions and handover with actual implementation and validation limits.
- [ ] Run serial acad-app/acad-cmd covering tests, fmt, workspace all-target Clippy; one serial non-oracle integration gate before commit. No QEMU reruns or Lean work for an app-only layout change.
- [ ] Self-review, commit, full task-1-report.md with commands/results/limitations. Coordinator independently reviews and verifies actual window typing/menu/error/unload before local merge, then merged non-oracle check and DIM resume/rebase without overwriting partial evidence.
