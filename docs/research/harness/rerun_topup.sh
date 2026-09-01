#!/bin/bash
# Atlas orchestration: after the kernel test (tmux opb3) finishes, apply the
# swap-off amendment, re-stage under policy, preflight, and re-run the
# N8b/AX pilot top-up under the amended gates.
export PATH="$HOME/.local/bin:$PATH"
while tmux has-session -t opb3 2>/dev/null; do sleep 30; done
echo "=== kernel test finished $(date -Is); applying swapoff + re-staging ==="
sudo bash ~/op-bench-harness/atlas_setup.sh
uv run ~/op-bench-harness/pilot_native.py stage || exit 1
uv run ~/op-bench-harness/pilot_native.py preflight || exit 1
uv run ~/op-bench-harness/pilot_native.py run --rounds 5 --cells N8b,AX --out ~/op-bench-pilot-topup2
echo "=== top-up rerun complete $(date -Is) ==="
