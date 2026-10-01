/-!
Finite observations from docs/recovery/2026-10-01-menu-controls/implementation.md.
Artifact paths below are relative to that evidence directory. `step` returns none
outside the listed before/event pairs: it is not a universal native interpreter.
Coordinates/spacing are exact decimal tokens, not a floating-point model. Buffer
is a symbolic pending payload: paced text is literal, a picked object uses id 1.
This does not assert the native selection buffer representation. Rust App.input
correspondence needs separate tests.
History.untracked means this projection makes no history claim. Status is the
message produced by this operation, not the accumulated native transcript.
No mouse projection, general error recovery, macro semantics or DWG codec proof.
-/
namespace AutoCAD.MenuControls

structure Point where
  x : String
  y : String
  deriving DecidableEq, Repr

inductive Item where
  | line (start finish : Point)
  | erasedLine (start finish : Point)
  | point (origin : Point)
  | circle (center : Point) (radius : String)
  deriving DecidableEq, Repr

inductive Prompt where
  | command | lineFirst | lineNext (anchor : Point)
  | circleRadius (center : Point) | erase | point | menuFile | repeatColumns
  deriving DecidableEq, Repr

inductive History where
  | untracked | fresh | menu | line | point
  deriving DecidableEq, Repr

structure State where
  snap : Bool := false
  spacing : String := "1.0"
  ortho : Bool := false
  prompt : Prompt := .command
  items : List Item := [] -- all recorded entities are on native layer 1
  repeatOpen : Bool := false -- opener only, not a decoded/completed REPEAT group
  panelPage : Option Nat := some 0
  buffer : String := ""
  history : History := .untracked
  status : String := ""
  deriving DecidableEq, Repr

inductive Event where
  | snap | ortho | cancel | go | returnKey
  | submitted (text : String)
  deriving DecidableEq, Repr

structure Recording where
  before : State
  event : Event
  after : State
  artifact : String
  deriving Repr

def p23 : Point := ⟨"2", "3"⟩
def p45 : Point := ⟨"4", "5"⟩
def p87 : Point := ⟨"8", "7"⟩
def p65 : Point := ⟨"6", "5"⟩
def fractionalAnchor : Point := ⟨"2.1", "3.15"⟩
def fractionalEnd : Point := ⟨"4.25", "5.15"⟩
def pickedStart : Point := ⟨"1.96078431372554", "3.58823529411769"⟩
def pickedEnd : Point := ⟨"11.7647058823534", "3.58823529411769"⟩
def picked : Item := .line pickedStart pickedEnd

def idle : State := {}
def off : State := { spacing := "0.5" }
def on : State := { off with snap := true, ortho := true }
def snapOn : State := { off with snap := true, status := "<Snap on>" }
def orthoOn : State := { off with ortho := true, status := "<Ortho on>" }
def fractionalLine : State := { off with prompt := .lineNext fractionalAnchor }
def snapLine : State := { fractionalLine with snap := true, status := "<Snap on>" }
def orthoLine : State := { fractionalLine with ortho := true, status := "<Ortho on>" }
def radius : State := { off with prompt := .circleRadius p23 }
def snapRadius : State := { radius with snap := true, status := "<Snap on>" }
def orthoRadius : State := { radius with ortho := true, status := "<Ortho on>" }
def doneLine : State := { idle with prompt := .lineNext p45, items := [.line p23 p45] }
def firstLine : State := { idle with prompt := .lineFirst }
def nextLine : State := { idle with prompt := .lineNext p23 }
def pendingCircle : State := { idle with prompt := .circleRadius p23 }
def selected : State := { idle with prompt := .erase, items := [picked], buffer := "1" }
def cancelled : State := { idle with status := "*Cancel*" }
def repeated : State := { idle with repeatOpen := true, items := [.point p45] }
def repeatCancelled : State := { repeated with status := "*Cancel*" }
def columns : State := { repeated with prompt := .repeatColumns }
def invalidColumns : State := { repeated with status := "*Invalid*" }
def fresh : State := { idle with history := .fresh }
def unknown : String := "Unknown command. Type ? for list of commands."
def menuHistory : State := { idle with history := .menu }
def menuPrompt : State := { menuHistory with prompt := .menuFile }
def lineHistory : State := { idle with history := .line }
def invalidFirst : State := { lineHistory with status := "*Invalid*" }
def pointHistory : State := { idle with history := .point, items := [.point p87] }
def selectedStatus : String := "1 selected, 1 found."

-- Every row is a recorded finite state; no wildcard event/state clause.
def recordings : List Recording := [
  ⟨off, .snap, snapOn, "pilot/captures/SCIDLE-027-probe.png"⟩,
  ⟨off, .ortho, orthoOn, "pilot/captures/OCIDLE-027-probe.png"⟩,
  ⟨snapOn, .snap, { off with status := "<Snap off>" },
    "controls/captures/SCTWICE-032-probe.png"⟩,
  ⟨orthoOn, .ortho, { off with status := "<Ortho off>" },
    "controls/captures/OCTWICE-032-probe.png"⟩,
  ⟨on, .snap, { on with snap := false, status := "<Snap off>" },
    "controls/drawings/SCON.decoded.txt"⟩,
  ⟨on, .ortho, { on with ortho := false, status := "<Ortho off>" },
    "controls/drawings/OCON.decoded.txt"⟩,
  ⟨fractionalLine, .snap, snapLine, "controls/captures/FSLINE-031-probe.png"⟩,
  ⟨fractionalLine, .ortho, orthoLine, "controls/captures/FOLINE-031-probe.png"⟩,
  ⟨snapLine, .submitted "4.25,5.15",
    { off with snap := true, prompt := .lineNext fractionalEnd, items := [.line fractionalAnchor fractionalEnd] },
    "controls/drawings/FSLINE.decoded.txt"⟩,
  ⟨orthoLine, .submitted "4.25,5.15",
    { off with ortho := true, prompt := .lineNext fractionalEnd, items := [.line fractionalAnchor fractionalEnd] },
    "controls/drawings/FOLINE.decoded.txt"⟩,
  ⟨radius, .snap, snapRadius, "controls/captures/SCIRCLE-031-probe.png"⟩,
  ⟨radius, .ortho, orthoRadius, "controls/captures/OCIRCLE-031-probe.png"⟩,
  ⟨snapRadius, .submitted "1.25",
    { off with snap := true, items := [.circle p23 "1.25"] },
    "controls/drawings/SCIRCLE.decoded.txt"⟩,
  ⟨orthoRadius, .submitted "1.25",
    { off with ortho := true, items := [.circle p23 "1.25"] },
    "controls/drawings/OCIRCLE.decoded.txt"⟩,
  ⟨idle, .cancel, cancelled, "cancel/captures/CIDLE-015-probe.png"⟩,
  ⟨doneLine, .cancel, { cancelled with items := [.line p23 p45] },
    "pilot/captures/CLDONE-021-probe.png"⟩,
  ⟨{ cancelled with items := [.line p23 p45] }, .submitted "POINT;8,7",
    { idle with items := [.line p23 p45, .point p87] },
    "pilot/drawings/CLDONE.decoded.txt"⟩,
  ⟨firstLine, .cancel, cancelled, "cancel/captures/CLSTART-017-probe.png"⟩,
  ⟨nextLine, .cancel, cancelled, "cancel/captures/CLSEG-019-probe.png"⟩,
  ⟨pendingCircle, .cancel, cancelled, "cancel/captures/CCENTER-019-probe.png"⟩,
  ⟨cancelled, .submitted "POINT;8,7", { idle with items := [.point p87] },
    "cancel/drawings/CLSEG.decoded.txt"⟩,
  ⟨selected, .cancel, { cancelled with items := [picked] },
    "pilot/drawings/CSELECT.decoded.txt"⟩,
  ⟨{ idle with buffer := "p" }, .cancel, cancelled,
    "cancel-tail/captures/CBIDLE-019-probe.png"⟩,
  ⟨{ firstLine with buffer := "2" }, .cancel, cancelled,
    "cancel-tail/captures/CBPOINT-021-probe.png"⟩,
  ⟨{ idle with prompt := .erase, buffer := "1" }, .cancel, cancelled,
    "cancel-tail/captures/CBSELECT-021-probe.png"⟩,
  ⟨{ firstLine with panelPage := some 1 }, .cancel,
    { cancelled with panelPage := some 1 }, "cancel-tail/captures/CPAGE1-022-probe.png"⟩,
  ⟨{ firstLine with panelPage := some 2 }, .cancel,
    { cancelled with panelPage := some 2 }, "cancel-tail/captures/CPAGE2-027-probe.png"⟩,
  ⟨repeated, .cancel, repeatCancelled, "cancel/captures/CREPEAT-021-probe.png"⟩,
  ⟨repeatCancelled, .submitted "ENDREP", columns,
    "cancel/captures/CREPEAT-023-continuation.png"⟩,
  ⟨columns, .submitted "REPEAT", invalidColumns,
    "cancel/captures/CREPEAT-025-continuation.png"⟩,
  ⟨invalidColumns, .submitted "POINT;6,5",
    { repeated with items := [.point p45, .point p65] }, "cancel/drawings/CREPEAT.dxf"⟩,
  ⟨{ repeated with items := [.point p45, .point p65] }, .submitted "ENDREP",
    { columns with items := [.point p45, .point p65] },
    "cancel/captures/CREPEAT-031-continuation.png"⟩,
  ⟨fresh, .go, { fresh with status := unknown }, "go/captures/GOFRESH-013-probe.png"⟩,
  ⟨fresh, .returnKey, { fresh with status := unknown },
    "go/captures/RTFRESH-010-probe.png"⟩,
  ⟨{ menuHistory with buffer := ";" }, .returnKey, { menuHistory with status := unknown },
    "pilot/captures/SEMICOL-018-continuation.png"⟩,
  ⟨menuHistory, .go, menuPrompt, "pilot/captures/GOIDLE-019-probe.png"⟩,
  ⟨menuHistory, .returnKey, menuPrompt, "pilot/captures/RETURN-014-probe.png"⟩,
  ⟨menuPrompt, .go, { menuHistory with panelPage := none },
    "pilot/captures/GOIDLE-026-continuation.png"⟩,
  ⟨menuPrompt, .cancel, { menuHistory with status := "*Cancel*" },
    "controls/captures/CMENU-020-probe.png"⟩,
  ⟨{ firstLine with history := .line }, .go, invalidFirst,
    "go/captures/GOFIRST-017-probe.png"⟩,
  ⟨{ nextLine with history := .line }, .go, lineHistory,
    "go/captures/GONEXT-019-probe.png"⟩,
  ⟨pendingCircle, .go, { idle with status := "*Invalid*" },
    "go/captures/GORADIUS-019-probe.png"⟩,
  ⟨{ idle with prompt := .erase, items := [.line p23 p45] }, .go,
    { idle with items := [.line p23 p45] }, "go/drawings/GOSELECT.decoded.txt"⟩,
  ⟨invalidFirst, .submitted "2,3", { lineHistory with status := unknown },
    "go/captures/GOFIRST-019-continuation.png"⟩,
  ⟨{ lineHistory with status := unknown }, .submitted "4,5",
    { lineHistory with status := unknown }, "go/captures/GOFIRST-021-continuation.png"⟩,
  ⟨lineHistory, .submitted "4,5", { lineHistory with status := unknown },
    "go/captures/GONEXT-021-continuation.png"⟩,
  ⟨{ lineHistory with status := unknown }, .returnKey,
    { firstLine with history := .line }, "go/captures/GOFIRST-023-continuation.png"⟩,
  ⟨pointHistory, .go, { pointHistory with prompt := .point },
    "go/captures/GOHIST-021-probe.png"⟩,
  ⟨{ firstLine with buffer := "2,3" }, .go, nextLine,
    "buffers/captures/GOBUFPNT-029-probe.png"⟩,
  ⟨nextLine, .submitted "4,5;Return", { idle with items := [.line p23 p45] },
    "buffers/drawings/GOBUFPNT.decoded.txt"⟩,
  ⟨selected, .go, { idle with items := [.erasedLine pickedStart pickedEnd], status := selectedStatus }, "buffers/captures/GOBUFSEL-036-probe.png"⟩,
  ⟨selected, .returnKey, { idle with items := [.erasedLine pickedStart pickedEnd], status := selectedStatus }, "buffers/captures/CSELBASE-033-probe.png"⟩,
  ⟨{ idle with items := [.erasedLine pickedStart pickedEnd], status := selectedStatus },
    .submitted "POINT;8,7",
    { idle with items := [.erasedLine pickedStart pickedEnd, .point p87] },
    "buffers/drawings/GOBUFSEL.decoded.txt"⟩
]

/-- Certified lookup only. The submitted strings containing `;` above denote
recorded multi-input continuation sequences; they do not specify macro splitting. -/
def step (before : State) (event : Event) : Option State :=
  (recordings.find? fun row => row.before == before && row.event == event).map (·.after)

-- SCIDLE, OCIDLE, SCTWICE, OCTWICE, SCON, OCON: idle settings + exact status.
example : step off .snap = some snapOn := by decide
example : step off .ortho = some orthoOn := by decide
example : step snapOn .snap = some { off with status := "<Snap off>" } := by decide
example : step orthoOn .ortho = some { off with status := "<Ortho off>" } := by decide
example : step on .snap = some { on with snap := false, status := "<Snap off>" } := by decide
example : step on .ortho = some { on with ortho := false, status := "<Ortho off>" } := by decide
-- FSLINE/FOLINE and SCIRCLE/OCIRCLE: toggles preserve pending values;
-- recorded typed continuations establish geometry, not just prompt strings.
example : step fractionalLine .snap = some snapLine := by decide
example : step fractionalLine .ortho = some orthoLine := by decide
example : step snapLine (.submitted "4.25,5.15") = some
    { off with snap := true, prompt := .lineNext fractionalEnd, items := [.line fractionalAnchor fractionalEnd] } := by decide
example : step orthoLine (.submitted "4.25,5.15") = some
    { off with ortho := true, prompt := .lineNext fractionalEnd, items := [.line fractionalAnchor fractionalEnd] } := by decide
example : step radius .snap = some snapRadius := by decide
example : step radius .ortho = some orthoRadius := by decide
example : step snapRadius (.submitted "1.25") = some
    { off with snap := true, items := [.circle p23 "1.25"] } := by decide
example : step orthoRadius (.submitted "1.25") = some
    { off with ortho := true, items := [.circle p23 "1.25"] } := by decide
-- CLDONE, CLSTART, CLSEG, CCENTER, CIDLE: completed versus pending geometry.
example : step doneLine .cancel = some { cancelled with items := [.line p23 p45] } := by decide
example : step firstLine .cancel = some cancelled := by decide
example : step nextLine .cancel = some cancelled := by decide
example : step pendingCircle .cancel = some cancelled := by decide
example : step idle .cancel = some cancelled := by decide
example : step cancelled (.submitted "POINT;8,7") = some { idle with items := [.point p87] } := by decide
example : step { cancelled with items := [.line p23 p45] } (.submitted "POINT;8,7") =
    some { idle with items := [.line p23 p45, .point p87] } := by decide
-- CSELECT, CBIDLE/CBPOINT/CBSELECT, CPAGE1/CPAGE2: selection, buffers, pages.
example : step selected .cancel = some { cancelled with items := [picked] } := by decide
example : step { idle with buffer := "p" } .cancel = some cancelled := by decide
example : step { firstLine with buffer := "2" } .cancel = some cancelled := by decide
example : step { idle with prompt := .erase, buffer := "1" } .cancel = some cancelled := by decide
example : step { firstLine with panelPage := some 1 } .cancel =
    some { cancelled with panelPage := some 1 } := by decide
example : step { firstLine with panelPage := some 2 } .cancel =
    some { cancelled with panelPage := some 2 } := by decide
-- CREPEAT: native prompts/DXF only; no full decoded group/header claim.
example : step repeated .cancel = some repeatCancelled := by decide
example : step repeatCancelled (.submitted "ENDREP") = some columns := by decide
example : step columns (.submitted "REPEAT") = some invalidColumns := by decide
example : step invalidColumns (.submitted "POINT;6,5") =
    some { repeated with items := [.point p45, .point p65] } := by decide
example : step { repeated with items := [.point p45, .point p65] } (.submitted "ENDREP") =
    some { columns with items := [.point p45, .point p65] } := by decide
-- GOFRESH/RTFRESH/SEMICOL, GOIDLE/RETURN/CMENU: Return-like, single submission.
example : step fresh .go = some { fresh with status := unknown } := by decide
example : step fresh .returnKey = some { fresh with status := unknown } := by decide
example : step { menuHistory with buffer := ";" } .returnKey =
    some { menuHistory with status := unknown } := by decide
example : step menuHistory .go = some menuPrompt := by decide
example : step menuHistory .returnKey = some menuPrompt := by decide
example : step menuPrompt .go = some { menuHistory with panelPage := none } := by decide
example : step menuPrompt .cancel = some { menuHistory with status := "*Cancel*" } := by decide
-- GOFIRST/GONEXT/GORADIUS/GOSELECT/GOHIST, including later LINE history.
example : step { firstLine with history := .line } .go = some invalidFirst := by decide
example : step { nextLine with history := .line } .go = some lineHistory := by decide
example : step pendingCircle .go = some { idle with status := "*Invalid*" } := by decide
example : step { idle with prompt := .erase, items := [.line p23 p45] } .go =
    some { idle with items := [.line p23 p45] } := by decide
example : step invalidFirst (.submitted "2,3") = some { lineHistory with status := unknown } := by decide
example : step { lineHistory with status := unknown } (.submitted "4,5") =
    some { lineHistory with status := unknown } := by decide
example : step lineHistory (.submitted "4,5") = some { lineHistory with status := unknown } := by decide
example : step { lineHistory with status := unknown } .returnKey =
    some { firstLine with history := .line } := by decide
example : step pointHistory .go = some { pointHistory with prompt := .point } := by decide
-- GOBUFPNT, GOBUFSEL/CSELBASE: do not replace buffered input with empty text.
example : step { firstLine with buffer := "2,3" } .go = some nextLine := by decide
example : step nextLine (.submitted "4,5;Return") =
    some { idle with items := [.line p23 p45] } := by decide
example : step selected .go = some { idle with items := [.erasedLine pickedStart pickedEnd], status := selectedStatus } := by decide
example : step selected .returnKey = some { idle with items := [.erasedLine pickedStart pickedEnd], status := selectedStatus } := by decide
example : step { idle with items := [.erasedLine pickedStart pickedEnd], status := selectedStatus }
    (.submitted "POINT;8,7") =
    some { idle with items := [.erasedLine pickedStart pickedEnd, .point p87] } := by decide
-- Unrecorded spacing/state/buffer transitions have no fidelity claim.
example : step { off with spacing := "0.25" } .snap = none := by decide
example : step { fractionalLine with buffer := "2" } .snap = none := by decide
example : step { idle with panelPage := some 1 } .go = none := by decide

end AutoCAD.MenuControls
