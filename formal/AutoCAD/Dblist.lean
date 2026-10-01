import AutoCAD.Wblock

/-!
The DBLIST read contract: report live entity records in drawing order, including
entities contained by blocks and repeats; erased records are absent. The exact
legacy text layout is intentionally outside this data-level model.
-/

namespace AutoCAD.Dblist

open AutoCAD.Wblock

def records (items : List Item) : List Entity :=
  items.flatMap fun item =>
    match item with
    | .entity entity => [entity]
    | .erased _ => []
    | .block block => block.entities
    | .repeat entities => entities

def ids (items : List Item) : List Nat := (records items).map Entity.id

example : ids AutoCAD.Wblock.sample.items = [1, 3, 2, 4, 5, 6] := by decide

example : (records AutoCAD.Wblock.sample.items).length = 6 := by decide

end AutoCAD.Dblist
