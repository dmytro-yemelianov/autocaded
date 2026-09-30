import Lake
open Lake DSL

package «autocad-formal»

@[default_target]
lean_lib AutoCAD where
  roots := #[`AutoCAD.Wblock, `AutoCAD.Dblist, `AutoCAD.Units, `AutoCAD.HelpFiles, `AutoCAD.Geometry]
