# QEMU oracle probe

`acad-oracle` can boot the original System floppy under `qemu-system-i386`,
send keys through QMP, and extract a DWG and DXF from a private copy of the
floppy.
Run the probe with:

```sh
cargo test -p acad-oracle -- --nocapture
```

The tests skip if the extracted images or QEMU are absent. The first selects
**New drawing**, names it `ORCEMPTY`, executes `END`, then selects **Make
drawing interchange file**. It checks the original's 640-byte `AC1.40` DWG
and compares the generated DXF, through DOS EOF, with the Rust DXF writer's
output after parsing it. The second mounts Samples in drive B and asks the
original to export `SELEXOL`, `BLIVET`, `FLOW`, `FLOOR`, `ADDER`, `HOUSE`,
`COLORS`, `OFFICE`, and `SHUTTLE`. Those fresh DXFs are compared with the
Rust DWG reader for both versions by block name, entity type, order within
each block, full header, and six-decimal field values. AutoCAD
exports block definitions in an order that can differ from their DWG order.
The floppies are copied under `/tmp`; corpus images are never modified. A
small in-tree FAT12 reader extracts the output.

`tests/commands.rs` exercises entity creation too: two `CIRCLE` commands
with different centers and radii, a `LINE` with fractional endpoints, an
`ARC` through three points, and rotated `TEXT`. The expectations come from the command
inputs and elementary geometry. It checks the complete entity list, then
compares the Rust DXF writer's output with the original's exported bytes
through DOS EOF. It also decodes the generated `AC1.40` DWGs and compares
their complete DXF representation with the original's exports. A separate
case changes BASE, limits, snap/grid spacing, ortho, fill, and two layer
colors, so defaults cannot conceal offset mistakes. The circle test rejects
every truncated prefix before the declared end of entity data.

These probes established the `AC1.40` entity start at `0x202`, direct TEXT
height (the `AC1.2` corpus requires a 0.75 conversion), BASE at `0x0C`, the
current layer at `0xC4`, and 128 `u16` layer colors at `0xC8`. They also
corrected FILL from `0xB0` to `0xB2`: both words were 1 in the old corpus,
so only a nondefault drawing exposed the error. As an independent AC1.2
check, setting just SUBDIV's word at `0xB2` to 0 on a disposable Samples
image made the original export `MODEFILL,1` followed by `0`.

The exports establish the DXF shapes of `POINT`, `TRACE`, `SOLID`, `REPEAT`,
and `ENDREP`. They also expose an earlier lexer error: the number in an
entity header such as `LINE,20` denotes its layer, not an instance count.
The model currently discards that per-entity layer; the comparison checks
geometry and structure, not layer fidelity. `REPEAT` and `ENDREP` markers are
also discarded after decoding their enclosed entities, so the test does not
establish the construct's full repeat semantics.

## QEMU 11.0.1 branch workaround

The harness passes `-d nochain` to disable TCG block chaining. Without it,
QEMU 11.0.1 can leave a wrapping 16-bit near call's EIP untruncated. During
`CIRCLE`, the instruction at `20C2:016A` (`E8 10 F8`) should call `F97D`
in the same segment. The CPU trace instead records `EIP=FFFFF97D`, and
execution enters physical address `2059D` instead of `3059D`. Running data
as code then corrupts memory; later symptoms include an invalid-opcode loop,
false resource errors, and AutoCAD aborting.

This matches the upstream report and fix,
[“fix EIP truncation for wrapping 16-bit near branches”](https://www.mail-archive.com/qemu-devel@nongnu.org/msg1224825.html).
Disabling chaining takes the path that masks EIP correctly. The command
tests pass with the workaround; removing it made the circle test time out
after the original aborted. No AutoCAD binary bytes are changed. Keep the
workaround until a QEMU version containing the fix is required and tested.

## Remaining limits

The reader still rejects font `LOAD` and `SHAPE` records, including the
`LOAD ROMAN-S` in `DISC.BAK`; DWG writing is not implemented. The harness
requires an external QEMU installation; it is not the spec's in-tree
8086 core. Editor input uses paced keystrokes; synchronization with visible
prompts currently covers the text-mode menus only.

AutoCAD draws its editor into CGA memory at `B800:0000` while QEMU displays
that memory as VGA text. The screen appears corrupted in a QEMU screenshot,
but the bitmap can be reconstructed from a 16 KiB memory dump: even scanlines
are 80-byte runs starting at offset 0, odd scanlines at offset `0x2000`, with
the high bit of each byte representing the leftmost pixel. The harness uses
the readable DOS text screen to synchronize at the menu and naming prompts.
