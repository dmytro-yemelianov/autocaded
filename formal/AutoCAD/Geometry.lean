/-!
Observed command contracts for `DIM`, `HATCH`, and `SKETCH` in AutoCAD 1.4.

QEMU confirms that DIM writes ordinary LINE, SOLID, and TEXT entities rather
than a dedicated dimension record. QEMU confirms HATCH LINE clipping for a
closed four-LINE loop, a CIRCLE, and a closed semicircle-plus-chord loop selected
by a window: it creates an
anonymous block of layer-127 pattern lines and inserts it on the current layer.
Rust matches the native default, scale-2 / 30-degree, circle, nested-hole, and
semicircle-plus-chord cases. The implementation is limited to closed LINE/ARC
loops, circles, and the LINE pattern. SKETCH proceeds from its record increment to digitizer input, so
it is hardware-dependent.

Superseded for SKETCH: the `unsupportedDevice` outcome below models only the
increment prompt. A QEMU mouse oracle (`crates/acad-oracle/tests/sketch_mouse.rs`)
shows that the original accepts a mouse after the increment, and the native editor
implements that contract (`docs/native-sketch.md`). The Lean transition is kept
unchanged and is not a claim about post-increment behaviour.
-/

namespace AutoCAD.Geometry

inductive Prompt where
  | firstExtensionOrigin
  | dimensionIntersection
  | secondExtensionOrigin
  | dimensionText
  | hatchPattern
  | hatchScale
  | hatchAngle
  | hatchSelection
  | hatchWindowFirst
  | hatchWindowSecond
  | sketchRecordIncrement
  deriving DecidableEq, Repr

inductive Phase where
  | command
  | dimFirstExtension
  | dimIntersection
  | dimSecondExtension
  | dimText
  | hatchPattern
  | hatchScale
  | hatchAngle
  | hatchSelection
  | hatchWindowFirst
  | hatchWindowSecond
  | sketchIncrement
  | sketchDigitizer
  deriving DecidableEq, Repr

inductive Input where
  | dim
  | hatch
  | sketch
  | point
  | text
  | pattern
  | question
  | scalar
  | selection
  | window
  | increment
  deriving DecidableEq, Repr

inductive Outcome where
  | prompt (value : Prompt)
  | dimensionEntities
  | hatchLineBlock
  | patternList
  | unsupportedGeometry
  | unsupportedDevice
  | invalid
  deriving DecidableEq, Repr

def advance : Phase → Input → Outcome
  | .command, .dim => .prompt .firstExtensionOrigin
  | .dimFirstExtension, .point => .prompt .dimensionIntersection
  | .dimIntersection, .point => .prompt .secondExtensionOrigin
  | .dimSecondExtension, .point => .prompt .dimensionText
  | .dimText, .text => .dimensionEntities
  | .command, .hatch => .prompt .hatchPattern
  | .hatchPattern, .question => .patternList
  | .hatchPattern, .pattern => .prompt .hatchScale
  | .hatchScale, .scalar => .prompt .hatchAngle
  | .hatchAngle, .scalar => .prompt .hatchSelection
  | .hatchSelection, .selection => .hatchLineBlock
  | .hatchSelection, .window => .prompt .hatchWindowFirst
  | .hatchWindowFirst, .point => .prompt .hatchWindowSecond
  | .hatchWindowSecond, .point => .hatchLineBlock
  | .command, .sketch => .prompt .sketchRecordIncrement
  | .sketchIncrement, .increment => .unsupportedDevice
  | _, _ => .invalid

/-! The DIM prompts and primitive-entity output are QEMU-observed. This contract
does not specify the geometry calculations or claim parity for every DIM mode.
-/

example : advance .command .dim = .prompt .firstExtensionOrigin := by decide
example : advance .dimFirstExtension .point = .prompt .dimensionIntersection := by decide
example : advance .dimIntersection .point = .prompt .secondExtensionOrigin := by decide
example : advance .dimSecondExtension .point = .prompt .dimensionText := by decide
example : advance .dimText .text = .dimensionEntities := by decide

example : advance .command .hatch = .prompt .hatchPattern := by decide
example : advance .hatchPattern .pattern = .prompt .hatchScale := by decide
example : advance .hatchScale .scalar = .prompt .hatchAngle := by decide
example : advance .hatchAngle .scalar = .prompt .hatchSelection := by decide
example : advance .hatchSelection .selection = .hatchLineBlock := by decide
example : advance .hatchSelection .window = .prompt .hatchWindowFirst := by decide
example : advance .hatchWindowFirst .point = .prompt .hatchWindowSecond := by decide
example : advance .hatchWindowSecond .point = .hatchLineBlock := by decide
example : advance .hatchPattern .question = .patternList := by decide
example : advance .command .sketch = .prompt .sketchRecordIncrement := by decide
example : advance .sketchIncrement .increment = .unsupportedDevice := by decide

end AutoCAD.Geometry
