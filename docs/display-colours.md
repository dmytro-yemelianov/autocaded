# Display colours

AutoCAD 1.4 stores a colour number (0–255) per layer (`LAYERC`). The retained
COLOR help page establishes only 1–7 (red, yellow, green, cyan, blue, magenta,
white). Everything else was decided by the display driver chosen at
configuration time, so the renderer offers two palettes
(`acad_model::Palette`, `crates/acad-model/src/color.rs`):

| Palette | Source |
|---|---|
| `pc16` (default) | The 1983 Tecmar Graphics Master driver, below. |
| `aci256` | The later (R12-era) 256-colour ACI table, `aci_rgb`. Not 1983 evidence. |

Choose with `acad --palette pc16|aci256`, `Session::set_palette`, or the
browser page's palette list.

## The drivers on the 1.4 disks

`corpus/Utilities/` holds the five display drivers shipped with 1.4:

| Driver | Board | Colour evidence |
|---|---|---|
| `DSIBMSS.DRV` | IBM CGA, single screen (the configured one: `ACAD.CFG` names `DSIBMSS`) | 640×200 monochrome: every colour is lit. |
| `DSIBMDS.DRV` | IBM CGA, dual screen | Same graphics mode. |
| `DSHERC.DRV` | Hercules | Monochrome. |
| `DSTECSS.DRV` | Tecmar Graphics Master (the board `System/README.DOC` documents) | 16 colours; mapping below. |
| `DSVTX.DRV` | Vectrix | Sends ASCII commands (`REC`, `ORC`, …) to the board's own processor through a host routine; the colour lookup is in the board, not the driver. |

## `DSTECSS.DRV` colour routine

Disassembled with `ndisasm -b16 corpus/Utilities/DSTECSS.DRV`. Data addresses
are DS-relative; file offset = DS offset + `0x5F4` (the only value that aligns
both tables below with their uses). The routine at file `0x0ADD` takes the
AutoCAD colour number in BX and returns the pixel byte in AL/AH:

```
0ADD  cmp bx,0 / jz 0AFD          ; colour 0 -> background colour [0C04]
0AE2  cmp bx,8 / jnl 0AED         ; colour >= 8 -> used as is
0AE7  add bx,bx
0AE9  mov bx,[bx+07A2]            ; colours 1..7 -> word table
0AED  and bx,000F                 ; hardware index = n & 15
0AF1  cmp bx,[0C04] / jnz 0B01
0AF7  mov bx,000F                 ; same as background -> 15 (white)
0B01  mov al,[bx+0792]            ; 00 11 22 .. FF: index in both nibbles
```

The word table at DS `07A2` (file `0x0D96`) is `0, 4, 14, 2, 3, 1, 5, 15`, so
colours 1–7 become IBM RGBI 4 (red), 14 (yellow), 2 (green), 3 (cyan), 1
(blue), 5 (magenta) and 15 (white). The driver writes only port `0x39A` (the
board's control register) and never loads a palette, so the indices are the
board's fixed RGBI colours.

`pc16` reproduces this with the IBM 5153 RGBI values (`AA` for normal, `55`
added for intensity, index 6 brown) on a black background:

| Colour | RGBI | RGB |
|---|---|---|
| 1 red | 4 | `AA0000` |
| 2 yellow | 14 | `FFFF55` |
| 3 green | 2 | `00AA00` |
| 4 cyan | 3 | `00AAAA` |
| 5 blue | 1 | `0000AA` |
| 6 magenta | 5 | `AA00AA` |
| 7 white | 15 | `FFFFFF` |
| 8–15 | n | grey `555555`, light blue, light green, light cyan, light red, light magenta, yellow, white |
| 16 and up | n & 15 | as above; results of 0 (16, 32, …) draw white |

Two consequences the earlier palettes got wrong: colour 15, the default layer
colour, is white rather than ACI 15's brown (127,63,63), and colours 9–14 are
RGBI 9–14 (9 is light *blue*), not bright variants of 1–6.

## Known differences

- **Colour 0.** The driver draws it in the background colour (an erase). The
  renderer draws it white, as a stand-in for BYBLOCK until block colour
  inheritance is modelled (owner decision 2026-10-05: keep white in both
  palettes).
- **White passes through.** `acad_render::flatten::style` treats white
  primitives as uncoloured, so white block contents take the insert layer's
  colour. With `pc16` that applies to colours 7, 15 and every multiple of 16;
  it is what makes `COLORS.DWG` (a `UNIT` block on colour 15 inserted on
  layers 1–64) read as a colour chart.
- **Monitor.** RGBI index 6 is brown on the IBM 5153 and dark yellow on some
  other RGBI monitors; colour numbers only reach 6 as `n & 15` of 22, 38, ….
- **Unverified on hardware.** QEMU cannot emulate a Graphics Master, so this
  rests on the driver code, not a captured frame.
