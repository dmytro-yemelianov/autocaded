# flake8: noqa

# Import all CAD verification functions to trigger registration
from .cad_audit import cad_oracle_function
from .cad_audit import lean_prover_function
from .cad_audit import cargo_verifier_function
from .cad_audit import ledger_status_function
from .cad_audit import corpus_checker_function
from .cad_audit import gui_api_checker_function
