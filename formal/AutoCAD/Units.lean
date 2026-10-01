/-!
Observable contract for AutoCAD 1.4 `RES`/`RESOLUTION`, `UNITS`, `DELAY`,
and `RESUME`.

Native help identifies RES and RESOLUTION as SNAP-resolution commands. QEMU
DWG snapshots establish that UNITS writes its format and precision at 0x1d8
and 0x1da respectively. DELAY and RESUME act on the script runner, leaving a
drawing snapshot unchanged.
-/

namespace AutoCAD.Units

inductive Format where
  | scientific | decimal | engineering | architectural
  deriving DecidableEq, Repr

structure Settings where
  snapEnabled : Bool
  snapSpacing : Int
  format : Format
  precision : Nat
  deriving DecidableEq, Repr

def validPrecision : Format → Nat → Bool
  | .architectural, value => value == 1 || value == 2 || value == 4 || value == 8 ||
      value == 16 || value == 32 || value == 64
  | _, value => value <= 8

def resolution (settings : Settings) (value : Int) : Option Settings :=
  if value <= 0 then none else some { settings with snapEnabled := true, snapSpacing := value }

def resolutionOff (settings : Settings) : Settings := { settings with snapEnabled := false }

def setUnits (settings : Settings) (format : Format) (precision : Nat) : Option Settings :=
  if validPrecision format precision then some { settings with format, precision } else none

def scriptControl (settings : Settings) : Settings := settings

def sample : Settings := ⟨false, 1, .decimal, 4⟩

example : resolution sample 5 = some ⟨true, 5, .decimal, 4⟩ := by decide
example : resolutionOff ⟨true, 5, .decimal, 4⟩ = ⟨false, 5, .decimal, 4⟩ := by decide
example : setUnits sample .decimal 3 = some ⟨false, 1, .decimal, 3⟩ := by decide
example : setUnits sample .architectural 16 = some ⟨false, 1, .architectural, 16⟩ := by decide
example : setUnits sample .architectural 3 = none := by decide
example : scriptControl sample = sample := by decide

end AutoCAD.Units
