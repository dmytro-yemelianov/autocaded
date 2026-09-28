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
original to export `SELEXOL`, `BLIVET`, `FLOW`, `FLOOR`, and `ADDER`. Those
fresh DXFs are compared with the Rust `AC1.2` DWG reader by block name,
entity type, order within each block, and six-decimal field values. AutoCAD
exports block definitions in an order that can differ from their DWG order.
The floppies are copied under `/tmp`; corpus images are never modified. A
small in-tree FAT12 reader extracts the output.

The exports establish the DXF shapes of `POINT`, `TRACE`, `SOLID`, `REPEAT`,
and `ENDREP`. They also expose an earlier lexer error: the number in an
entity header such as `LINE,20` denotes its layer, not an instance count.
The model currently discards that per-entity layer; the comparison checks
geometry and structure, not layer fidelity. `REPEAT` and `ENDREP` markers are
also discarded after decoding their enclosed entities, so the test does not
establish the construct's full repeat semantics.

## Current limit

The sample exports verify geometry already present on the original floppies,
but the harness does not yet verify entity-creation command semantics.
Sending `CIRCLE` after entering the editor
leaves the original at `Command: CIRCLE` and it does not return to the menu
after center, radius, and `END` are sent. In one run QEMU repeatedly
reported `CS:IP = 20C2:0D6C`; bytes at that physical address were `FF`.
Adding the `DSIBMSS.DRV`, `DGMS.DRV`, and `PLHP.DRV` files named in
`ACAD.CFG` from the Utilities disk to a disposable System image did not
resolve it. This does not establish whether the remaining problem is QEMU's
hardware model, the disk arrangement, or the input sequence.

The original writes `AC1.40`, while the shipped DWG reader currently supports
only `AC1.2`. Both entity-command execution and `AC1.40` decoding are needed
before this path can verify a generated drawing against the Rust model.

AutoCAD draws its editor into CGA memory at `B800:0000` while QEMU displays
that memory as VGA text. The screen appears corrupted in a QEMU screenshot,
but the bitmap can be reconstructed from a 16 KiB memory dump: even scanlines
are 80-byte runs starting at offset 0, odd scanlines at offset `0x2000`, with
the high bit of each byte representing the leftmost pixel. The harness uses
the readable DOS text screen to synchronize at the menu and naming prompts.
