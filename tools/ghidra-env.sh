#!/usr/bin/env bash
# tools/ghidra-env.sh — locate Ghidra + JDK 21 and build the PyGhidra venv.
#
#   source tools/ghidra-env.sh   # exports GHIDRA_INSTALL_DIR, JAVA_HOME, PYGHIDRA_PYTHON
#   ./tools/ghidra-env.sh        # diagnose only; non-zero if anything is missing
set -euo pipefail
root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
venv="$root/build/pyghidra-venv"

die() { echo "ghidra-env: $*" >&2; return 1; }

find_ghidra() {
  if [ -n "${GHIDRA_INSTALL_DIR:-}" ]; then
    if [ -d "$GHIDRA_INSTALL_DIR/Ghidra" ]; then
      echo "$GHIDRA_INSTALL_DIR"; return
    fi
    # Falling back is right — a stale variable should not block you — but doing
    # it silently would analyse a different Ghidra than the one you named.
    echo "ghidra-env: GHIDRA_INSTALL_DIR=$GHIDRA_INSTALL_DIR has no Ghidra/ inside; searching instead" >&2
  fi
  # ${HOME:-} guards set -u: a missing HOME must still reach the message below,
  # not abort with "unbound variable".
  for c in /opt/homebrew/Cellar/ghidra/*/libexec /usr/local/Cellar/ghidra/*/libexec \
           /opt/ghidra /usr/share/ghidra "${HOME:-/nonexistent}/ghidra"; do
    [ -d "$c/Ghidra" ] && { echo "$c"; return; }
  done
  die "Ghidra not found. Install it (macOS: brew install ghidra) or set GHIDRA_INSTALL_DIR."
}

find_jdk() {
  for c in "${JAVA_HOME:-}" /opt/homebrew/opt/openjdk@21 /usr/local/opt/openjdk@21 \
           /usr/lib/jvm/java-21-openjdk; do
    [ -n "$c" ] && [ -x "$c/bin/java" ] && { echo "$c"; return; }
  done
  die "JDK 21 not found. Ghidra 12 needs it (macOS: brew install openjdk@21) or set JAVA_HOME."
}

find_python() {
  for c in python3.13 python3.12 python3.11; do
    command -v "$c" >/dev/null && { command -v "$c"; return; }
  done
  die "Python 3.11+ not found. The bundled JPype has no wheel for older versions."
}

GHIDRA_INSTALL_DIR="$(find_ghidra)"
JAVA_HOME="$(find_jdk)"
py="$(find_python)"
export GHIDRA_INSTALL_DIR JAVA_HOME

dist="$GHIDRA_INSTALL_DIR/Ghidra/Features/PyGhidra/pypkg/dist"
[ -d "$dist" ] || die "no PyGhidra wheels at $dist — is this Ghidra 11.3 or newer?"

if [ ! -x "$venv/bin/python" ]; then
  echo "ghidra-env: creating $venv" >&2
  "$py" -m venv "$venv"
  "$venv/bin/pip" install --quiet --upgrade pip
  "$venv/bin/pip" install --quiet --no-index --find-links "$dist" pyghidra
fi
export PYGHIDRA_PYTHON="$venv/bin/python"

# Sourced or run? `return` succeeds only when sourced.
(return 0 2>/dev/null) || {
  echo "GHIDRA_INSTALL_DIR = $GHIDRA_INSTALL_DIR"
  echo "JAVA_HOME          = $JAVA_HOME ($("$JAVA_HOME/bin/java" -version 2>&1 | head -1))"
  echo "PYGHIDRA_PYTHON    = $PYGHIDRA_PYTHON"
  "$PYGHIDRA_PYTHON" -c "import pyghidra; print('pyghidra', pyghidra.__version__)"
}
