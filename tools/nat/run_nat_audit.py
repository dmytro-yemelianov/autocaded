#!/usr/bin/env python3
"""Run the AutoCAD 1.4 Native Verification Agent using NVIDIA NeMo Agent Toolkit (NAT)."""
import os
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
NAT_BIN = ROOT.parent / "kit/.venv/bin/nat"
CONFIG = ROOT / "tools/nat/cad_audit/src/cad_audit/configs/config.yml"


def main():
    if not NAT_BIN.is_file():
        print(f"Error: NAT binary not found at {NAT_BIN}", file=sys.stderr)
        sys.exit(1)
    if not CONFIG.is_file():
        print(f"Error: NAT config not found at {CONFIG}", file=sys.stderr)
        sys.exit(1)

    input_prompt = " ".join(sys.argv[1:]) if len(sys.argv) > 1 else "Verify all formal proofs and check cargo workspace status."
    cmd = [
        str(NAT_BIN),
        "run",
        "--config_file",
        str(CONFIG),
        "--input",
        input_prompt,
    ]
    print(f"Executing NAT workflow: {' '.join(cmd)}")
    result = subprocess.run(cmd, cwd=ROOT)
    sys.exit(result.returncode)


if __name__ == "__main__":
    main()
