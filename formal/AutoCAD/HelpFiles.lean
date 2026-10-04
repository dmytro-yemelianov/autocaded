/-!
Observable contracts for AutoCAD 1.4's `?`, `HELP`, `MENU`, and `FILES`
commands. These state transitions come from QEMU recordings. They deliberately
stop before the File Utility's list/delete/rename operations and before menu
file parsing, which need separate filesystem contracts.
-/

namespace AutoCAD.HelpFiles

inductive Prompt where
  | commandName
  | menuFile
  deriving DecidableEq, Repr

inductive Page where
  | commandList
  | commandHelp (name : String)
  | fileUtility
  deriving DecidableEq, Repr

inductive State where
  | command
  | helpQuery
  | menuQuery
  deriving DecidableEq, Repr

inductive Outcome where
  | prompt (value : Prompt)
  | page (value : Page)
  | command
  | invalid
  deriving DecidableEq, Repr

def knownCommands : List String := [
  "LINE", "POINT", "CIRCLE", "ARC", "TRACE", "SOLID", "TEXT", "SHAPE",
  "INSERT", "BLOCK", "WBLOCK", "REPEAT", "ENDREP", "ARRAY", "HATCH",
  "FILLET", "BREAK", "SKETCH", "ZOOM", "PAN", "MOVE", "COPY", "ERASE",
  "OOPS", "CHANGE", "LAYER", "GRID", "SNAP", "AXIS", "ORTHO", "LIMITS",
  "UNITS", "AREA", "DIST", "ID", "LIST", "DBLIST", "STATUS", "REDRAW",
  "REGEN", "DIM", "DELAY", "RESUME", "MENU", "FILES", "END", "QUIT",
  "HELP", "PLOT", "QPLOT", "TABLET", "FILL", "RES", "RESOLUTION", "?"
]

def advance : State → String → Outcome
  | .command, "?" => .prompt .commandName
  | .command, "HELP" => .prompt .commandName
  | .command, "MENU" => .prompt .menuFile
  | .command, "FILES" => .page .fileUtility
  | .command, _ => .invalid
  | .helpQuery, "" => .page .commandList
  | .helpQuery, name =>
      if knownCommands.contains name then
        .page (.commandHelp name)
      else
        .invalid
  | .menuQuery, _ => .command

example : advance .command "?" = .prompt .commandName := by rfl
example : advance .command "HELP" = .prompt .commandName := by rfl
example : advance .helpQuery "" = .page .commandList := by rfl
example : advance .helpQuery "LINE" = .page (.commandHelp "LINE") := by decide
example : advance .helpQuery "CIRCLE" = .page (.commandHelp "CIRCLE") := by decide
example : advance .helpQuery "FOOBAR" = .invalid := by decide
example : advance .command "MENU" = .prompt .menuFile := by rfl
example : advance .command "FILES" = .page .fileUtility := by rfl

end AutoCAD.HelpFiles
