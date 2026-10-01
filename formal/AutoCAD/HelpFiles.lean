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

def advance : State → String → Outcome
  | .command, "?" => .prompt .commandName
  | .command, "HELP" => .prompt .commandName
  | .command, "MENU" => .prompt .menuFile
  | .command, "FILES" => .page .fileUtility
  | .command, _ => .invalid
  | .helpQuery, "" => .page .commandList
  | .helpQuery, "LINE" => .page (.commandHelp "LINE")
  | .helpQuery, _ => .invalid
  | .menuQuery, _ => .command

example : advance .command "?" = .prompt .commandName := by rfl
example : advance .command "HELP" = .prompt .commandName := by rfl
example : advance .helpQuery "" = .page .commandList := by rfl
example : advance .helpQuery "LINE" = .page (.commandHelp "LINE") := by rfl
example : advance .helpQuery "CIRCLE" = .invalid := by rfl
example : advance .command "MENU" = .prompt .menuFile := by rfl
example : advance .command "FILES" = .page .fileUtility := by rfl

end AutoCAD.HelpFiles
