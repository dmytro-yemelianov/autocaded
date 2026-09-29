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

The backup test exports `DISC.BAK` by changing only its directory extension
on the disposable Samples copy before boot. It compares the complete drawing,
including `LOAD ROMAN-S`, the following SHUTTLE insert, `LOAD ITALIC`, and
the following STAR WARS text in their original order. An existing DWG with
the same name is an error, so backup promotion cannot overwrite a drawing.

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

`tests/shapes.rs` mounts Samples in drive B and issues `LOAD B:ES`, followed
by `SHAPE RES` and `SHAPE CAP` with different positions, heights, and nonzero
rotations. ES.SHP independently supplies definition numbers 129 and 130.
The test checks those inputs against the original DXF and compares decoded
DWG output byte for byte, then rejects every truncated DWG prefix. Loading
TXT and trying its glyph names did not create SHAPE records; the successful
probe uses the dedicated ES shape library.

LOAD's binary body is a length-prefixed name; its DXF body is one name row.
SHAPE's binary body is four doubles (x, y, scale, angle in radians), then a
u16 definition number. DXF uses one row of x,y,scale,angle-in-degrees,number.
These new record layouts are verified for AC1.40; the AC1.2 corpus contains
neither LOAD nor SHAPE, so their older-version layouts are not independently verified.

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

## Interactive runner

`cargo run -p acad-oracle --example acad-qemu` boots the original with
System in drive A and Samples in drive B and shows it in a window. The disks
are working copies in `target/acad-qemu/`, so drawings survive between runs;
`--fresh` recopies them from the corpus. Keys are forwarded as QMP key
events. Guest Shift, Ctrl, and Alt also follow the host's modifier flags,
and numpad digits type digits. Unmapped host keys are reported on stderr.

QEMU's own display cannot show the editor: `DSIBMSS` programs the CGA mode
register at `3D8h`, which QEMU's VGA ignores. It cannot be trusted for text
either: after `END`, AutoCAD restores text mode with CGA CRTC values, which
leave QEMU's VGA drawing 8-line character cells. The runner therefore reads
the 16 KiB at `B8000h` about 15 times a second, decides from its contents
whether the guest is in text or graphics mode (`acad_oracle::cga::detect`),
and renders it as a CGA would: 80×25 cells using the BIOS's 8×8 font at
`F000:FA6E`, or the 640×200 bitmap with its status line, screen menu, and
prompt area. The text cursor is not drawn. The mouse is not connected yet;
the configured `DGMS` driver expects a Mouse Systems serial mouse.

## Remaining limits

LOAD and SHAPE records survive both codecs, and the renderer now interprets
the supplied SHP libraries. `tests/font_render.rs` captures the original CGA
memory and compares text/shape strokes; see [font rendering](shp-rendering.md).
DWG writing remains open. The harness
requires an external QEMU installation; it is not the spec's in-tree
8086 core. Editor input uses paced keystrokes; synchronization with visible
prompts covers the text-mode menus. Visual probes additionally wait for the
BIOS keyboard queue to drain and CGA memory to stabilize before capturing.

AutoCAD draws its editor into CGA memory at `B800:0000` while QEMU displays
that memory as VGA text. The screen appears corrupted in a QEMU screenshot,
but the bitmap can be reconstructed from a 16 KiB memory dump: even scanlines
are 80-byte runs starting at offset 0, odd scanlines at offset `0x2000`, with
the high bit of each byte representing the leftmost pixel. The harness uses
the readable DOS text screen to synchronize at the menu and naming prompts.
