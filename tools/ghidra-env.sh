#!/usr/bin/env bash
# tools/ghidra-env.sh — locate Ghidra + JDK 21 and build the PyGhidra venv.
#
#   source tools/ghidra-env.sh   # exports GHIDRA_INSTALL_DIR, JAVA_HOME, PYGHIDRA_PYTHON
#   ./tools/ghidra-env.sh        # diagnose only; non-zero if anything is missing
set -euo pipefail
# ${BASH_SOURCE[0]:-$0} so this can be sourced from zsh, where BASH_SOURCE does
# not exist and `set -u` would make reading it fatal. nullglob so an unmatched
# search glob below expands to nothing instead of aborting under zsh's nomatch.
setopt nullglob 2>/dev/null || shopt -s nullglob
root="$(cd "$(dirname "${BASH_SOURCE[0]:-$0}")/.." && pwd)"
venv="$root/build/pyghidra-venv"

# Ghidra 12 requires a JDK 21 or newer.
MIN_JDK=21

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

# Major version of a JDK, or nothing if it will not report one.
jdk_major() {
  "$1/bin/java" -version 2>&1 | sed -n '1s/.*version "\([0-9]*\).*/\1/p' | head -1
}

find_jdk() {
  # An explicit JAVA_HOME is honoured only if it is new enough. A stale one
  # pointing at an older JDK must not shadow an installed openjdk@21 — the
  # failure then lands as a Java stack trace from pyghidra.start(), which is
  # exactly what this script exists to prevent.
  for c in "${JAVA_HOME:-}" /opt/homebrew/opt/openjdk@21 /usr/local/opt/openjdk@21 \
           /usr/lib/jvm/java-21-openjdk; do
    [ -n "$c" ] && [ -x "$c/bin/java" ] || continue
    major="$(jdk_major "$c")"
    if [ -n "$major" ] && [ "$major" -ge "$MIN_JDK" ] 2>/dev/null; then
      echo "$c"; return
    fi
    if [ "$c" = "${JAVA_HOME:-}" ]; then
      echo "ghidra-env: JAVA_HOME=$c is Java ${major:-unknown}, need $MIN_JDK+; searching instead" >&2
    fi
  done
  die "JDK $MIN_JDK+ not found. Ghidra 12 needs it (macOS: brew install openjdk@21) or set JAVA_HOME."
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
