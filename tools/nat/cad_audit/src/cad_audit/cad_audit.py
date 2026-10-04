import logging
import subprocess
from pathlib import Path
from pydantic import Field

from nat.plugin_api import Builder
from nat.plugin_api import FunctionBaseConfig
from nat.plugin_api import FunctionInfo
from nat.plugin_api import LLMFrameworkEnum
from nat.plugin_api import register_function

logger = logging.getLogger(__name__)
REPO_ROOT = Path(__file__).resolve().parents[5]


class CadOracleToolConfig(FunctionBaseConfig, name="cad_oracle_runner"):
    """Tool to execute AutoCAD 1.4 QEMU/in-tree oracle verification suites."""
    pass


@register_function(config_type=CadOracleToolConfig, framework_wrappers=[LLMFrameworkEnum.LANGCHAIN])
async def cad_oracle_function(config: CadOracleToolConfig, builder: Builder):
    async def _run_oracle(test_name: str = "") -> str:
        """Run an oracle test against the original 1983 AutoCAD 1.4 image to establish ground truth.

        Args:
            test_name: Optional test name or filter (e.g. 'empty_repeat', 'repeat_layer').

        Returns:
            The oracle execution output and verification status.
        """
        cmd = ["cargo", "test", "-p", "acad-oracle"]
        if test_name:
            cmd.extend(["--test", test_name])
        proc = subprocess.run(cmd, cwd=REPO_ROOT, capture_output=True, text=True)
        if proc.returncode == 0:
            return f"ORACLE PASS: {proc.stdout.splitlines()[-1] if proc.stdout else 'ok'}"
        return f"ORACLE FAIL (code {proc.returncode}):\n{proc.stdout}\n{proc.stderr}"

    yield FunctionInfo.from_fn(_run_oracle, description=_run_oracle.__doc__)


class LeanProverToolConfig(FunctionBaseConfig, name="lean_prover"):
    """Tool to compile and verify Lean 4 formal specifications."""
    pass


@register_function(config_type=LeanProverToolConfig, framework_wrappers=[LLMFrameworkEnum.LANGCHAIN])
async def lean_prover_function(config: LeanProverToolConfig, builder: Builder):
    async def _check_proofs(target: str = "") -> str:
        """Compile and check all Lean 4 formal proofs in formal/ using lake build.

        Args:
            target: Optional target module or empty for all.

        Returns:
            Proof compilation output and theorem validation results.
        """
        formal_dir = REPO_ROOT / "formal"
        cmd = ["lake", "build"]
        if target:
            cmd.append(target)
        proc = subprocess.run(cmd, cwd=formal_dir, capture_output=True, text=True)
        if proc.returncode == 0:
            return f"LEAN FORMAL PROOFS PASS: All targets built and verified successfully."
        return f"LEAN FORMAL PROOFS FAIL:\n{proc.stderr}\n{proc.stdout}"

    yield FunctionInfo.from_fn(_check_proofs, description=_check_proofs.__doc__)


class CargoVerifierToolConfig(FunctionBaseConfig, name="cargo_verifier"):
    """Tool to check Rust compiler warnings, formatting, and clippy lints."""
    pass


@register_function(config_type=CargoVerifierToolConfig, framework_wrappers=[LLMFrameworkEnum.LANGCHAIN])
async def cargo_verifier_function(config: CargoVerifierToolConfig, builder: Builder):
    async def _verify_cargo(package: str = "") -> str:
        """Run cargo clippy and format checks across the entire workspace.

        Args:
            package: Optional package name to verify or empty for all.

        Returns:
            Compiler and lint status.
        """
        cmd = ["cargo", "clippy", "--workspace", "--all-targets", "--", "-D", "warnings"]
        if package:
            cmd = ["cargo", "clippy", "-p", package, "--all-targets", "--", "-D", "warnings"]
        proc = subprocess.run(cmd, cwd=REPO_ROOT, capture_output=True, text=True)
        if proc.returncode == 0:
            return "CARGO WORKSPACE CLEAN: 0 clippy warnings."
        return f"CARGO CLIPPY WARNINGS/ERRORS:\n{proc.stderr}"

    yield FunctionInfo.from_fn(_verify_cargo, description=_verify_cargo.__doc__)


class LedgerStatusToolConfig(FunctionBaseConfig, name="ledger_status"):
    """Tool to inspect the implementation loop progress ledger."""
    pass


@register_function(config_type=LedgerStatusToolConfig, framework_wrappers=[LLMFrameworkEnum.LANGCHAIN])
async def ledger_status_function(config: LedgerStatusToolConfig, builder: Builder):
    async def _check_ledger(command: str = "status") -> str:
        """Inspect the native completion ledger status and validation gates.

        Args:
            command: Subcommand for progress ledger (e.g. 'status', 'summary').

        Returns:
            Summary of verified and pending implementation tasks.
        """
        cmd = ["python3", "tools/agentic_progress.py", command or "status"]
        proc = subprocess.run(cmd, cwd=REPO_ROOT, capture_output=True, text=True)
        return proc.stdout or proc.stderr

    yield FunctionInfo.from_fn(_check_ledger, description=_check_ledger.__doc__)
