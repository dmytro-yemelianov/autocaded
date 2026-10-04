/-!
Observable WBLOCK semantics for the native AutoCAD 1.4 rebuild.

This is a behavioral specification, not a claim that Ghidra recovered the
original command implementation. Prompt order and export behavior are modeled
from the recorded QEMU interaction; the Rust editor and DWG codec refine it.
-/

namespace AutoCAD.Wblock

structure Point where
  x : Int
  y : Int
  deriving DecidableEq, Repr

structure Entity where
  id : Nat
  selectable : Bool := true
  blockRefs : List Nat := []
  deriving DecidableEq, Repr

structure Block where
  id : Nat
  base : Point
  entities : List Entity
  deriving DecidableEq, Repr

inductive Item where
  | entity : Entity → Item
  | erased : Entity → Item
  | block : Block → Item
  | repeat : List Entity → Item
  deriving DecidableEq, Repr

structure Drawing where
  base : Point
  items : List Item
  deriving DecidableEq, Repr

structure Snapshot where
  base : Point
  items : List Item
  deriving DecidableEq, Repr

inductive Request where
  | whole
  | named (id : Nat)
  | selected (entityNumbers : List Nat) (base : Point)
  deriving DecidableEq, Repr

inductive Prompt where
  | outputPath
  | blockName
  | insertionBase
  | entitySelection
  deriving DecidableEq, Repr

inductive Phase where
  | command
  | path
  | name (path : Nat)
  | base (path : Nat)
  | selection (path : Nat) (base : Point)
  deriving DecidableEq, Repr

inductive Input where
  | wblock
  | path (value : Nat)
  | blankName
  | wholeDrawing
  | blockId (value : Nat)
  | point (value : Point)
  | entities (numbers : List Nat)
  deriving DecidableEq, Repr

inductive Outcome where
  | prompt (value : Prompt)
  | save (path : Nat) (snapshot : Snapshot)
  | invalid
  deriving DecidableEq, Repr

def unique (names : List Nat) : List Nat :=
  names.foldl (fun acc name => if acc.contains name then acc else acc ++ [name]) []

def blockRefsInItems (items : List Item) : List Nat :=
  items.flatMap fun item =>
    match item with
    | .entity entity => entity.blockRefs
    | .repeat entities => entities.flatMap Entity.blockRefs
    | .erased _ | .block _ => []

def blockRefsInDefinitions (blocks : List Block) (names : List Nat) : List Nat :=
  names.flatMap fun name =>
    match blocks.find? (fun block => block.id == name) with
    | some block => block.entities.flatMap Entity.blockRefs
    | none => []

def reachableBlocks (items : List Item) : List Nat := Id.run do
  let blocks := items.filterMap fun item =>
    match item with
    | .block block => some block
    | _ => none
  let rec close : Nat → List Nat → List Nat
    | 0, names => names
    | fuel + 1, names =>
        let grown := unique (names ++ blockRefsInDefinitions blocks names)
        if grown == names then names else close fuel grown
  return close (blocks.length + 1) (unique (blockRefsInItems items))

def keepWholeItem (reachable : List Nat) : Item → Bool
  | .entity _ => true
  | .erased _ => false
  | .block block => reachable.contains block.id
  | .repeat _ => true

def exportWhole (drawing : Drawing) : Snapshot :=
  let reachable := reachableBlocks drawing.items
  { base := drawing.base
    items := drawing.items.filter (keepWholeItem reachable) }

def findBlock (items : List Item) (id : Nat) : Option Block :=
  (items.find? fun item =>
    match item with
    | .block block => block.id == id
    | _ => false).bind fun item =>
      match item with
      | .block block => some block
      | _ => none

def exportNamed (drawing : Drawing) (id : Nat) : Option Snapshot := do
  let block ← findBlock drawing.items id
  let bodyItems := block.entities.map Item.entity
  let allBlocks := drawing.items.filter fun item => match item with | .block _ => true | _ => false
  let reachable := reachableBlocks (bodyItems ++ allBlocks)
  let neededBlocks := drawing.items.filterMap fun item =>
    match item with
    | .block b => if reachable.contains b.id then some (Item.block b) else none
    | _ => none
  pure { base := block.base, items := bodyItems ++ neededBlocks }

def selectableCount (items : List Item) : Nat :=
  items.foldl (fun count item =>
    match item with
    | .entity entity => if entity.selectable then count + 1 else count
    | _ => count) 0

def selectItems (items : List Item) (numbers : List Nat) : List Item := Id.run do
  let wanted := numbers.foldl (fun acc number => if acc.contains number then acc else acc ++ [number]) []
  let rec walk : List Item → Nat → List Item
    | [], _ => []
    | .entity entity :: rest, seen =>
        if entity.selectable then
          if wanted.contains (seen + 1) then
            .entity entity :: walk rest (seen + 1)
          else
            walk rest (seen + 1)
        else
          walk rest seen
    | _ :: rest, seen => walk rest seen
  return walk items 0

def exportSelected (drawing : Drawing) (numbers : List Nat) (base : Point) : Option Snapshot :=
  if numbers.isEmpty || numbers.any (fun number => number == 0 || number > selectableCount drawing.items) then
    none
  else
    let selected := selectItems drawing.items numbers
    let allBlocks := drawing.items.filter fun item => match item with | .block _ => true | _ => false
    let reachable := reachableBlocks (selected ++ allBlocks)
    let neededBlocks := drawing.items.filterMap fun item =>
      match item with
      | .block b => if reachable.contains b.id then some (Item.block b) else none
      | _ => none
    some { base := base, items := selected ++ neededBlocks }

def evaluate (drawing : Drawing) : Request → Option Snapshot
  | .whole => some (exportWhole drawing)
  | .named id => exportNamed drawing id
  | .selected numbers base => exportSelected drawing numbers base

def advance (drawing : Drawing) : Phase → Input → Outcome
  | .command, .wblock => .prompt .outputPath
  | .path, .path _ => .prompt .blockName
  | .name _path, .blankName => .prompt .insertionBase
  | .name path, .wholeDrawing => .save path (exportWhole drawing)
  | .name path, .blockId id =>
      match exportNamed drawing id with
        | some snapshot => .save path snapshot
        | none => .invalid
  | .base _path, .point _ => .prompt .entitySelection
  | .selection path base, .entities numbers =>
      match exportSelected drawing numbers base with
      | some snapshot => .save path snapshot
      | none => .invalid
  | _, _ => .invalid

/-! A small closed corpus of checked semantics, including transitive INSERT
references, erased records, skipped non-selectable entities, and a nonzero base.
-/

def p0 : Point := ⟨0, 0⟩
def pBase : Point := ⟨7, -3⟩
def e1 : Entity := ⟨1, true, [1]⟩
def e2 : Entity := ⟨2, true, []⟩
def eLoad : Entity := ⟨3, false, []⟩
def eA : Entity := ⟨4, true, [2]⟩
def eB : Entity := ⟨5, true, []⟩
def bA : Block := ⟨1, ⟨2, 1⟩, [eA]⟩
def bB : Block := ⟨2, ⟨0, 0⟩, [eB]⟩
def bOrphan : Block := ⟨3, p0, [⟨6, true, []⟩]⟩
def sample : Drawing := ⟨p0, [
  .entity e1,
  .entity eLoad,
  .erased ⟨90, true, []⟩,
  .entity e2,
  .block bA,
  .block bB,
  .block bOrphan
]⟩

example : exportWhole sample = ⟨p0, [
  .entity e1, .entity eLoad, .entity e2, .block bA, .block bB
]⟩ := by decide

example : exportNamed sample 1 = some ⟨bA.base, [.entity eA, .block bB]⟩ := by decide

example : exportNamed sample 99 = none := by decide

example : exportSelected sample [2] pBase = some ⟨pBase, [.entity e2]⟩ := by decide

example : exportSelected sample [1] pBase = some ⟨pBase, [.entity e1, .block bA, .block bB]⟩ := by decide

example : exportSelected sample [0] pBase = none := by decide

example : exportSelected sample [4] pBase = none := by decide

example : advance sample .command .wblock = .prompt .outputPath := by decide

example : advance sample .path (.path 1) = .prompt .blockName := by decide

example : advance sample (.name 1) .blankName = .prompt .insertionBase := by decide

example : advance sample (.name 1) .wholeDrawing = .save 1 (exportWhole sample) := by decide

example : advance sample (.name 1) (.blockId 1) =
    .save 1 ⟨bA.base, [.entity eA, .block bB]⟩ := by decide

example : advance sample (.base 1) (.point pBase) = .prompt .entitySelection := by decide

example : advance sample (.selection 1 pBase) (.entities [2]) =
    .save 1 ⟨pBase, [.entity e2]⟩ := by decide

example : advance sample (.selection 1 pBase) (.entities [1]) =
    .save 1 ⟨pBase, [.entity e1, .block bA, .block bB]⟩ := by decide

end AutoCAD.Wblock
