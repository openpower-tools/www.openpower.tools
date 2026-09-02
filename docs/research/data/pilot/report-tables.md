## SmolLM2-360M (the 360-million-parameter model) — prompt-reading speed (512-token prompt)

| configuration | measurements | rounds | median speed (tokens/s) | round-to-round spread | spread inside one round |
|---|---|---|---|---|---|
| N1 | 15 | 5 | 6.12 | 0.06% | 0.01% |
| N2 | 40 | 4 | 12.23 | 0.01% | 0.00% |
| N4 | 50 | 5 | 24.22 | 0.08% | 0.04% |
| N8 | 50 | 5 | 47.58 | 0.02% | 0.01% |
| N12 | 40 | 4 | 70.81 | 0.01% | 0.01% |
| N18 | 50 | 5 | 100.12 | 1.89% | 1.22% |
| P4 | 50 | 5 | 24.23 | 0.06% | 0.01% |
| P8 | 50 | 5 | 47.57 | 0.07% | 0.03% |
| S4x2 | 40 | 4 | 34.00 | 0.01% | 0.03% |
| S4x4 | 40 | 4 | 43.49 | 0.01% | 0.03% |
| S8x2 | 50 | 5 | 67.56 | 0.01% | 0.02% |
| N8b | 50 | 5 | 47.58 | 0.05% | 0.01% |
| AX | 50 | 5 | 198.69 | 0.06% | 0.09% |

plot: plots/pilot-m1-pp512.svg

## SmolLM2-360M (the 360-million-parameter model) — generation speed (writing 128 tokens)

| configuration | measurements | rounds | median speed (tokens/s) | round-to-round spread | spread inside one round |
|---|---|---|---|---|---|
| N1 | 50 | 5 | 5.46 | 0.05% | 0.03% |
| N2 | 40 | 4 | 10.44 | 0.01% | 0.01% |
| N4 | 50 | 5 | 20.49 | 0.13% | 0.02% |
| N8 | 50 | 5 | 36.71 | 0.02% | 0.02% |
| N12 | 40 | 4 | 48.33 | 0.02% | 0.02% |
| N18 | 40 | 4 | 59.40 | 0.19% | 0.18% |
| P4 | 50 | 5 | 20.55 | 0.01% | 0.01% |
| P8 | 50 | 5 | 37.38 | 0.10% | 0.11% |
| S4x2 | 40 | 4 | 24.77 | 0.05% | 0.05% |
| S4x4 | 40 | 4 | 24.89 | 0.11% | 0.02% |
| S8x2 | 50 | 5 | 38.93 | 0.06% | 0.03% |
| N8b | 50 | 5 | 38.11 | 0.02% | 0.02% |
| AX | 50 | 5 | 93.42 | 0.21% | 0.35% |

plot: plots/pilot-m1-tg128.svg

## Qwen3-0.6B (the 750-million-parameter model) — prompt-reading speed (512-token prompt)

| configuration | measurements | rounds | median speed (tokens/s) | round-to-round spread | spread inside one round |
|---|---|---|---|---|---|
| N1 | 15 | 5 | 8.54 | 0.19% | 0.02% |
| N2 | 50 | 5 | 17.06 | 1.16% | 0.30% |
| N4 | 50 | 5 | 34.07 | 0.02% | 0.02% |
| N8 | 50 | 5 | 66.09 | 0.30% | 0.09% |
| N12 | 50 | 5 | 94.49 | 0.01% | 0.01% |
| N18 | 50 | 5 | 133.81 | 0.03% | 0.04% |
| P4 | 50 | 5 | 34.08 | 0.03% | 0.04% |
| P8 | 50 | 5 | 66.09 | 0.08% | 0.02% |
| S4x2 | 50 | 5 | 43.08 | 0.09% | 0.05% |
| S4x4 | 50 | 5 | 53.15 | 0.02% | 0.05% |
| S8x2 | 50 | 5 | 85.07 | 0.03% | 0.05% |
| N8b | 50 | 5 | 66.10 | 0.02% | 0.04% |
| AX | 50 | 5 | 251.81 | 0.32% | 0.22% |

plot: plots/pilot-m2-pp512.svg

## Qwen3-0.6B (the 750-million-parameter model) — generation speed (writing 128 tokens)

| configuration | measurements | rounds | median speed (tokens/s) | round-to-round spread | spread inside one round |
|---|---|---|---|---|---|
| N1 | 50 | 5 | 6.37 | 0.15% | 0.05% |
| N2 | 50 | 5 | 12.24 | 0.13% | 0.15% |
| N4 | 50 | 5 | 23.21 | 0.02% | 0.03% |
| N8 | 50 | 5 | 41.43 | 0.75% | 0.10% |
| N12 | 50 | 5 | 58.20 | 0.04% | 0.06% |
| N18 | 50 | 5 | 77.27 | 0.07% | 0.12% |
| P4 | 50 | 5 | 23.33 | 0.03% | 0.01% |
| P8 | 50 | 5 | 42.15 | 0.27% | 0.12% |
| S4x2 | 50 | 5 | 25.83 | 0.12% | 0.04% |
| S4x4 | 50 | 5 | 26.73 | 0.15% | 0.02% |
| S8x2 | 50 | 5 | 47.83 | 0.07% | 0.03% |
| N8b | 50 | 5 | 42.95 | 0.02% | 0.03% |
| AX | 50 | 5 | 89.51 | 1.15% | 0.51% |

plot: plots/pilot-m2-tg128.svg

