#!/bin/bash
# Kernel-efficiency test for the former carried anomaly (descriptive,
# NON-decisional; not a registered cell). Requantises both generative
# models to pure Q4_0 / Q5_0 / Q8_0 (--allow-requantize; accuracy is
# irrelevant here, only bytes streamed per token and the kernel path) and
# measures tg128 at t=1 (CPU 72) and t=8 (N8 list) under the same pinning
# discipline as the pilot. Output: tok/s and effective GB/s per variant, so
# the per-byte efficiency of each POWER9 VSX dot-product kernel is visible.
# Run on atlas when no registered cell is executing.
set -u
export PATH="$HOME/.local/bin:$PATH"
Q=~/op-ask-spike/llama.cpp/build/bin/llama-quantize
B=~/op-ask-spike/llama.cpp/build/bin/llama-bench
OUT=/dev/shm/opb/kernel
mkdir -p "$OUT"
echo "=== kernel-efficiency test $(date -Is) ==="
for src in m1 m2; do
  for ty in Q4_0 Q5_0 Q8_0; do
    f="$OUT/$src-$ty.gguf"
    if [ ! -f "$f" ]; then
      numactl --membind=0 "$Q" --allow-requantize "/dev/shm/opb/node0/$src.gguf" "$f" "$ty" > "$OUT/$src-$ty.quantize.log" 2>&1 \
        || { echo "$src $ty: quantize FAILED (see $OUT/$src-$ty.quantize.log)"; continue; }
    fi
    size=$(stat -c %s "$f")
    for t in 1 8; do
      if [ "$t" = 1 ]; then cpus=72; mask=0x1000000000000000000
      else cpus=72,80,88,96,104,112,120,128; mask=0x101010101010101000000000000000000; fi
      numactl --membind=0 --physcpubind=$cpus "$B" -m "$f" -t "$t" -C "$mask" --cpu-strict 1 \
        -p 0 -n 128 -r 3 -o json 2>/dev/null > "$OUT/$src-$ty-t$t.json"
      python3 - "$src" "$ty" "$t" "$size" "$OUT/$src-$ty-t$t.json" <<'PY'
import json, sys
src, ty, t, size, path = sys.argv[1], sys.argv[2], sys.argv[3], int(sys.argv[4]), sys.argv[5]
r = json.load(open(path))[0]
print(f"{src} {ty} t={t} bytes={size} tg={r['avg_ts']:.2f} tok/s -> {r['avg_ts']*size/1e9:.2f} GB/s")
PY
    done
  done
done
echo "=== kernel test done $(date -Is) ==="
