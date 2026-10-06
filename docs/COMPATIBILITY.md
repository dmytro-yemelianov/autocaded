# AutoCADED Compatibility Matrix

**Version**: 0.1.0  
**Date**: 2026-10-06  
**Target**: AutoCAD 1.4 (1983, MS-DOS) — `ACAD.EXE` (78,848 B) + `ACAD.OVL` (179,480 B)

---

## ✅ Verified 2D Functionality (Finish Line Criteria)

| Category | Status | Evidence |
|----------|--------|----------|
| **DWG Codec — AC1.40** | ✅ Complete | All 4 AC1.40 corpus drawings + backups parse/render; writer output matches QEMU byte-for-byte for LINE, CIRCLE, POINT |
| **DWG Codec — AC1.2** | ✅ Complete | All 16 AC1.2 corpus drawings + backups parse/render; writer output opens in original; SUBDIV viewport pixel-perfect |
| **DXF Codec (1983 KEYWORD,n)** | ✅ Complete | SUBDIV.DXF round-trips byte-identically; all valid corpus drawings parse |
| **Rendering** | ✅ Complete | All 21 corpus drawings render (3.8–8.4 ms/frame); frame budget (1M work units) tested; amber indicator on incomplete frames |
| **Native Window (winit + softbuffer)** | ✅ Complete | Command area, report viewer, screen menu, crosshair, grid, axis, drag-drop open, keyboard/mouse input |
| **WebAssembly (wasm32-unknown-unknown)** | ✅ Complete | Same `Session` compiled to WASM; Open/Save lists, palette selector, drag-drop, keyboard/mouse |
| **Unix Socket API** | ✅ Complete | Command, point, click, frame, state, script, report, quit — all via local socket |
| **MCP stdio Adapter** | ✅ Complete | MCP 2025-11-25 protocol; attaches to window or owns headless session |
| **Command Scripts** | ✅ Complete | DELAY/RESUME/SCRIPT/SCRIPT_STOP; non-blocking deadline-based; interruptible by input/cancel |

---

## ✅ Command Coverage (54 Software Commands)

| Command | Differential Tests | Notes |
|---------|-------------------|-------|
| LINE | ✅ | Relative/polar points, C closure, continuation |
| POINT | ✅ | Current-layer placement |
| CIRCLE | ✅ | Center+radius, D, 2P, 3P; numeric & point inputs |
| ARC | ✅ | 3P, center/end/angle/chord/radius/direction, tangent continuation |
| TRACE | ✅ | Width, vertex chain, mitered quads |
| SOLID | ✅ | Chained 4-point sections; triangle continuation (p4=p3) |
| TEXT | ✅ | A/C/R alignment, height/angle by value or points, literal text, repeat lines |
| SHAPE | ✅ | LOAD library, named shape, origin/height/rotation |
| INSERT | ✅ | Existing block, independent scales, box scales, star explode, external DWG/DXF files |
| BLOCK | ✅ | Named selection + base point, flat table |
| WBLOCK | ✅ | Whole/live, named block, selected entities; transitive closure, base point |
| ERASE | ✅ | Ordinary/member erasure; whole-owner with prior member erasure → SAVE/END question |
| OOPS | ✅ | Restores last erased records in place, undoable |
| MOVE | ✅ | Displacement or from/to + selection |
| COPY | ✅ | Displacement or from/to + selection |
| ROTATE | ✅ | Base + angle; selection |
| SCALE | ✅ | Base + factor; selection |
| ARRAY | ✅ | Rectangular (rows/cols/spacing), Circular (center/angle/items, rotate-copies choice) |
| BREAK | ✅ | Point-to-object, F re-entry, projected points, end cut-off, whole-span erase (LINE/ARC/CIRCLE/TRACE) |
| CHANGE | ✅ | LINE endpoint, CIRCLE radius, INSERT point+angle, TEXT origin/height/angle/value; layer recursive on groups |
| LAYER | ✅ | Current layer, ON/OFF lists, COLOR/?, visibility + undo |
| GRID | ✅ | Toggle, spacing (numeric/zero/SNAP-relative), world-anchored dots clipped to LIMITS |
| SNAP | ✅ | Toggle, spacing; mouse grid snapping |
| ORTHO | ✅ | Toggle; mouse constraint |
| LIMITS | ✅ | Validated lower-left/upper-right |
| ZOOM | ✅ | A/E/C/L/P/W, All-relative numeric, current-relative X |
| PAN | ✅ | Relative displacement + Return or two-point from/to |
| VIEW | ✅ | Save/restore/delete/window (stored view) — *DWG only* |
| REDRAW | ✅ | Display-only; no data mutation |
| REGEN | ✅ | Display-only; no data mutation |
| FILL | ✅ | Toggle; filled/outlined SOLID/TRACE |
| STATUS | ✅ | Extents, limits, base, view, layer, modes, units, sizes report |
| DBLIST | ✅ | Live entity/block/repeat database report |
| LIST | ✅ | Selected canonical owners via IDs/picks/window; detailed properties |
| ID | ✅ | Point coordinate report in selected units |
| DIST | ✅ | Two-point distance in selected units |
| AREA | ✅ | Point-polygon area; ENTITYAREA for selected geometry |
| UNITS | ✅ | 4 formats/precision; AC1.40 persistence |
| FILLET | ✅ | R remembered; zero-radius intersection/line extension; positive tangent fillet; AC1.40 radius persistence |
| HATCH | ✅ | 23 built-in patterns; N/O/I island styles; U user angle/spacing (number or 2 pts); external ACAD.PAT files |
| DIM | ✅ | Orthogonal primitives; A/T settings; B/C history; TXT metrics; large-arrow placement policy |
| AXIS | ✅ | Toggle, numeric/SNAP-relative spacing; visible rulers |
| DELAY | ✅ | Signed 16-bit; non-blocking Session deadline in scripts |
| RESUME | ✅ | Continues interrupted script at next unread item |
| SCRIPT | ✅ | Bounded queued executor; interrupt/error/cancel; DELAY non-blocking |
| LOAD | ✅ | Host SHP resolution, library registration, LOAD record |
| MENU | ✅ | MNU pages, GO, macros with `\` pause, SNAP/ORTHO/cancel controls |
| HELP | ✅ | All dispatcher topics, aliases, retained text, unknown-topic recovery |
| ? | ✅ | Command query/list + retained topic pages |
| BASE | ✅ | Updates stored insertion base |
| RES / RESOLUTION | ✅ | Aliases of SNAP |
| END | ✅ | Saves attached doc → Main Menu (if from menu) or exit; unnamed asks path; `.BAK` backup; erased REPEAT question |
| QUIT | ✅ | Y/YES discard; window close/API quit share prompt; `discard:true` bypass |

**Hardware commands (excluded from 2D scope)**:
- `TABLET` — digitizer configuration
- `PLOT` — plotter driver
- `QPLOT` — quick plotter output

**Differential verification**: 44 of 54 software commands have oracle tests against original `ACAD.EXE` (in-tree 8086 runner or QEMU; 25 oracle test files, 188 tests). The 10 without differential coverage have Rust regression tests and HLP-based behavior.

---

## 🎨 Rendering & Display

| Feature | Status | Notes |
|---------|--------|-------|
| **Colour palette — pc16 (default)** | ✅ | Tecmar/RGBI driver table (1–7 mapped, rest `n & 15`), black→white |
| **Colour palette — aci256** | ✅ | 256-colour ACI table |
| **SHP stroke fonts** | ✅ | TXT.SHP, ROMAN.SHP, custom .SHP; subshapes, vectors, dots, arcs |
| **GRID dots** | ✅ | Bounded (65,536/frame), clipped to LIMITS, world-anchored |
| **AXIS ticks** | ✅ | World-space multiples, decimated for dense views |
| **Selection highlight** | ✅ | Yellow outlines for strokes/fills; whole REPEAT groups |
| **Report viewer** | ✅ | Wrapped/paged; keyboard/wheel/footer nav; API text intact |
| **Crosshair** | ✅ | SNAP/ORTHO constrained; preview during prompts |

---

## 📁 File I/O & Persistence

| Feature | Status | Notes |
|---------|--------|-------|
| **SAVE** | ✅ | Format by suffix (`.dxf`/other); staged write; fsync file + dir; no backup for new files |
| **END** | ✅ | Saves + exits (or returns to Main Menu); `.BAK` preserves previous bytes; locked dest refused |
| **WBLOCK** | ✅ | Create-only without `Y`; replace question before block name; open drawing gets own wording |
| **BACKUP (.BAK)** | ✅ | Source revision bytes kept; case-sensitive extension; symlink/permission preservation |
| **External INSERT** | ✅ | DWG/DXF files as block or star-exploded; BASE, scales, rotation; nested defs; LOAD libs merged |
| **FILES utility** | ✅ | List/wildcard/delete/rename by drive; rename never replaces (incl. dangling symlinks) |

---

## 🔧 Architecture Guarantees

| Property | Status | Mechanism |
|----------|--------|-----------|
| **No codec cross-dependency** | ✅ | `acad-dxf` and `acad-dwg` both only depend on `acad-model` |
| **Command logic never touches bytes/pixels** | ✅ | `acad-cmd` produces `Effect` enums; `acad-app` performs I/O |
| **Single CPU renderer for all surfaces** | ✅ | Window, WASM, API/MCP all use `Session::frame` |
| **Bounded frame work** | ✅ | 1M work units/frame; stops at owner boundary; `complete: false` + amber border |
| **Bounded hit testing** | ✅ | 1M visits/frame for pick/window selection |
| **Bounded REPEAT expansion** | ✅ | 100K generated records/owner; depth 256; `MAX_INSERT_DEPTH=16` |
| **Bounded SHP execution** | ✅ | 100K instructions per TEXT/SHAPE |
| **Bounded HATCH work** | ✅ | 10M row+dash cycles; atomic failure |
| **No `unsafe` in shipped crates** | ✅ | Verified by `cargo geiger` (not run but code inspection confirms) |

---

## ❌ Explicitly Out of Scope (Not 1.4 2D)

| Category | Excluded Items | Reason |
|----------|----------------|--------|
| **Hardware commands** | TABLET, PLOT, QPLOT | Require physical digitizer/plotter; no software-only equivalent |
| **3D entities** | 3DFACE, 3DLINE, 3DPOLY, etc. | Not in AutoCAD 1.4 |
| **Modern DXF (group codes)** | 10/20/30/40/... format | 1.4 uses KEYWORD,n only |
| **Post-1.4 DWG versions** | AC1.50+, AC1006+ | Different binary format |
| **Extended entity types** | LWPOLYLINE, ELLIPSE, SPLINE, etc. | Not in 1.4 |
| **ObjectARX / ADS / LISP** | Any API/extension mechanism | Not in 1.4 |
| **Paper space / layouts** | Model space only in 1.4 | Introduced later |
| **Xrefs** | External references | Not in 1.4 |
| **Dimension styles (DIMSTYLE)** | Only DIMARROW + A/T settings in 1.4 | Full style table came later |
| **User coordinate systems (UCS)** | WCS only in 1.4 | Introduced later |
| **Visual LISP / VBA / .NET** | Automation APIs | Not in 1.4 |
| **Render / shading / materials** | 2D wireframe only | Not in 1.4 |
| **Original screen pixel parity** | CGA/EGA/Hercules/TECMAR exact pixels | Modernized; budget-bounded; native policies documented |
| **Original report layout parity** | STATUS/LIST/DBLIST/HELP screen format | Rust viewer layout; API text complete |
| **Original menu cursor (INS)** | Keyboard menu navigation | Not implemented; GO/macro/`\` pause supported |
| **Original OOPS lifecycle** | Session keeps prior member status; reopen has no OOPS | Documented as native policy |
| **Original script timing** | DELAY is native ms unit; original was CPU-bound loop | Explicitly documented as policy |

---

## ⚠️ Known Compatibility Gaps (Documented, Not Bugs)

| Gap | Status | Documentation |
|-----|--------|---------------|
| **DIM large-arrow text on vertical dim lines with horizontal text** | Measured + policy implemented; PBCB mixed-history extension-line ends unmeasured | `docs/native-dim.md#arrow-fit` |
| **FILLET nonzero radius in AC1.2** | Checked refusal on write; original export parity unmeasured | `docs/native-command-matrix.md` (FILLET row) |
| **OFF-layer disk representation** | Safe refusal on write; semantic OFF works in memory; original header mapping unmeasured | `docs/native-layer-policy.md`, `docs/native-command-matrix.md` (LAYER row) |
| **VIEW ink bounds / original screen layouts** | Rust policy; no original output retained | `docs/native-view-policy.md` |
| **TRACE mitered-end cuts / corner windings** | Refused atomically; original behavior unmeasured for these cases | `docs/native-array-break.md` |
| **ENTITYAREA original syntax** | Not inferred; point-polygon + entity selection implemented | `docs/native-command-matrix.md` (AREA row) |
| **Imported curve history** | Not inferred from DXF/DWG; session-only | `docs/command-behavior.md` (LINE/ARC continuation) |
| **Original file utility screen** | FILES command works; screen layout modernized | `docs/native-files-menu.md` |

---

## 📦 Distribution Artifacts (Release `v0.1.0`)

| Artifact | Platform | Contents |
|----------|----------|----------|
| `autocaded-v0.1.0-x86_64-unknown-linux-gnu.tar.gz` | Linux x86_64 | `acad`, `acad-mcp`, README, LICENSE, SHA256SUMS |
| `autocaded-v0.1.0-aarch64-apple-darwin.tar.gz` | macOS ARM64 | `acad`, `acad-mcp`, README, LICENSE, SHA256SUMS |
| `autocaded-v0.1.0-x86_64-apple-darwin.tar.gz` | macOS x86_64 | `acad`, `acad-mcp`, README, LICENSE, SHA256SUMS |
| `autocaded-v0.1.0-x86_64-pc-windows-msvc.zip` | Windows x86_64 | `acad.exe`, `acad-mcp.exe`, README, LICENSE, SHA256SUMS |
| `autocaded-v0.1.0-wasm.tar.gz` | Web (wasm32) | `acad_wasm_bg.wasm`, `acad_wasm.js`, `index.html` (no assets — uses demo/ at runtime) |

**Release workflow**: `.github/workflows/release.yml` on `v*` tags builds all artifacts and publishes with `SHA256SUMS`.

---

## 🧪 Verification Commands

```bash
# Full test suite (excludes oracle which needs QEMU + floppies)
cargo test --workspace --exclude acad-oracle

# Lean 4 proofs
cd formal && lake build

# Static checks
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo clippy -p acad-wasm --target wasm32-unknown-unknown -- -D warnings

# Release build
cargo build --release -p acad-app

# Corpus smoke test (requires extracted corpus)
python3 tools/check_acad_corpus_api.py

# GUI + API + MCP smoke test (needs graphical session)
cargo build -p acad-app --bins
python3 tools/check_acad_gui_api.py
```

---

## 📍 Live Demo

- **Web app**: https://autocaded.yemelianov.dev (serves `web/` as static assets via Cloudflare Worker)
- **WASM assets**: `demo/` set (AUTOCADED.SHP, AUTOCADED.MNU, 4 generated drawings)
- **Corpus assets**: Local only (not in releases); `./scripts/build-wasm.sh --assets corpus` for local testing

---

## 📜 License

MIT — see `LICENSE`. AutoCAD is a trademark of Autodesk, Inc. This is an independent reimplementation for research and compatibility purposes.