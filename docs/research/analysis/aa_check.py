#!/usr/bin/env python3
"""Registered A/A calibration gate for benchmark-protocol.md (rev 3, section 4).

Runs the paired-ratio model on two aliases of the SAME configuration and
applies the registered acceptance criterion: the 94% HDI of the A/A
log-ratio must lie wholly within +/- log(1.02). Reports tau_b, tau_r,
sigma_w and nu beside pilot expectations for the manifest.

Usage:
  uv run aa_check.py data.jsonl --metric tg --cell-a AA1 --cell-b AA2 \
      --guess 30 [--seed 1] [--smoke]
Exit status 0 = gate passed; 1 = gate failed (diagnose before any A/B).
"""

import json
import subprocess
import sys


def main():
    args = sys.argv[1:]
    proc = subprocess.run(
        [sys.executable, __file__.replace("aa_check.py", "analyze.py"),
         "ratio", *args, "--rope", "1.02"],
        capture_output=True, text=True,
    )
    if proc.returncode != 0:
        sys.stderr.write(proc.stderr)
        sys.exit(2)
    result = json.loads(proc.stdout)
    result["aa_gate_passed"] = bool(result["hdi_within_rope"] and result["gates_ok"])
    print(json.dumps(result, indent=2))
    sys.exit(0 if result["aa_gate_passed"] else 1)


if __name__ == "__main__":
    main()
