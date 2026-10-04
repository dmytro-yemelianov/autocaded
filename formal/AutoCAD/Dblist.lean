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
    | .repeat _ members => members.filterMap fun m => if m.erased then none else some m.entity

def ids (items : List Item) : List Nat := (records items).map Entity.id

example : ids AutoCAD.Wblock.sample.items = [1, 3, 2, 4, 5, 6] := by decide

example : (records AutoCAD.Wblock.sample.items).length = 6 := by decide

/-! REPEAT group with explicit owner layer (E2) and erased members (B4) -/
def groupSample : List Item := [
  .entity ⟨10, true, []⟩,
  .repeat 2 [
    ⟨⟨11, true, []⟩, false⟩,
    ⟨⟨99, true, []⟩, true⟩,
    ⟨⟨12, true, []⟩, false⟩
  ],
  .entity ⟨13, true, []⟩
]

example : ids groupSample = [10, 11, 12, 13] := by decide

example : (records groupSample).length = 4 := by decide

theorem records_nil : records [] = [] := by rfl

theorem records_erased_empty (e : Entity) : records [.erased e] = [] := by rfl

theorem records_entity_singleton (e : Entity) : records [.entity e] = [e] := by rfl

end AutoCAD.Dblist
