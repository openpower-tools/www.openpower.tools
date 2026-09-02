# How fast do small language models run on our POWER9 server? Practice-run report

This is the report of a **practice run** (a "pilot") for a larger,
pre-registered benchmarking experiment. Its job was to shake out the
measurement rig and find out how noisy the measurements are, so the real
experiment is sized correctly. **Nothing in here decides anything** — the
binding rules and decision criteria live in
`docs/research/benchmark-protocol.md`, and the real experiment's data will
be collected fresh.

## What was measured, on what

The op-ask assistant for www.openpower.tools will run small language
models locally. We want to know, with evidence, how fast that is on POWER
hardware. The test machine ("atlas") is a two-socket POWER9 server: two
processor chips, each with 18 cores, each core able to run up to 4
hardware threads, and each socket with its own bank of RAM attached
(memory reachable from the other socket is slower — this locality is
called NUMA).

Two models were measured, as byte-identical files everywhere:

- **SmolLM2-360M** — 360 million parameters (the smaller model)
- **Qwen3-0.6B** — 750 million parameters (the larger model)

Two speeds were measured, both in **tokens per second** (a token is a word
fragment; roughly 3/4 of an English word):

- **prompt-reading speed** ("pp512"): how fast the model ingests a
  512-token prompt before it can answer;
- **generation speed** ("tg128"): how fast it writes out a 128-token
  answer. For feel: 5 tokens/s is about human reading pace.

## What the configuration names mean

Each measurement ran under a named CPU-and-memory configuration
("cell"), pinned exactly — specific cores, specific memory bank:

| name | plain meaning |
|---|---|
| N1, N2, N4, N8, N12, N18 | 1 to 18 cores on one socket, one thread per core, cores chosen so no two share a cache slice, memory on the same socket |
| P4, P8 | same core counts, but neighbouring cores that share cache ("packed") — tests whether cache sharing hurts |
| S4x2, S4x4, S8x2 | hardware-thread variants: e.g. S4x2 = 4 cores running 2 threads each (8 threads total) |
| N8b | the mirror image of N8 on the other socket — a sanity check that both halves of the machine behave alike |
| AX | all 36 cores across both sockets, with the model's memory striped evenly across both RAM banks |

## How to read the numbers

- Every configuration was measured in **5 separate rounds** spread across
  the night, with the order re-shuffled each round (so slow drift in the
  machine cannot systematically favour one configuration). Each round
  contains about 10 repetitions.
- **median speed** — the middle value of all repetitions; a typical run.
- **round-to-round spread** — how much the per-round averages differ,
  as a percentage of the overall average. This captures variation across
  hours. 0.1% means two rounds typically agree to one part in a thousand.
- **spread inside one round** — how much back-to-back repetitions differ.
- A **discard** is a run the rig rejected automatically because one of
  its self-checks failed (for example, the model's memory was not where
  the configuration said it must be); every discard is recorded with its
  cause, and the run is redone.

## What we learned

1. **The rig is very quiet.** Almost every single-socket configuration
   repeats to within 0.01-0.3%, across hours. The worst case in the whole
   table is 1.89% (SmolLM2 prompt-reading on all 18 cores, where one round
   of five ran about 3% slow — that configuration works the chip hardest,
   so its power/heat management has the most room to vary). Consequence:
   the sample sizes planned for the real experiment are comfortably
   sufficient.
2. **The scary-looking noise was a bug, not the hardware.** The
   both-sockets configuration (AX) initially varied 7-12% between rounds.
   The cause was memory quietly ending up on the wrong socket (details
   below); after the fix, its variation fell to 0.2-1.2%.
3. **The two sockets are not identical.** The second socket is
   consistently about 4% faster than the one we test on, at identical
   settings (N8b vs N8). Both sockets are healthy; the campaign keeps
   testing on the slower one and will report this asymmetry beside any
   cross-socket claim.
4. **Cache sharing between neighbouring cores costs nothing** at these
   model sizes: the "packed" configurations (P4, P8) exactly match the
   spread-out ones (N4, N8).
5. **Hardware threads help a little, then not at all.** Two threads per
   core buys +21% generation speed on 4 cores; four threads per core adds
   nothing more; and on 8 cores two-threads-per-core only buys +6% —
   because by then the limit is how fast RAM can feed the cores, not the
   cores themselves.
6. **How speed scales.** Prompt-reading scales almost perfectly with core
   count (it is compute-limited): SmolLM2 goes 6 -> 198 tokens/s from 1 to
   36 cores. Generation stops improving beyond roughly 12-18 cores on one
   socket (it is limited by memory bandwidth — how many bytes/second RAM
   can stream); using both sockets' RAM banks nearly doubles it. Note the
   models swap places at the top end: generation speed is set by bytes
   streamed per token, so when bandwidth is the bottleneck the smaller
   model wins (93 vs 90 tokens/s), while at low core counts the larger
   model is actually faster — which brings us to:
7. **A mystery, solved and measured.** All along, the 750M model has
   out-run the 360M model at low core counts, which looks impossible.
   The cause is how the models' weights are stored. Weights are kept
   compressed ("quantised") in named formats; there are two families —
   a newer one that compresses numbers in blocks of 256 (the "K"
   formats), and an older one using blocks of 32. The 360M model's
   internal width is 960, which does not divide by 256 — so the
   conversion tool silently stored most of it (80% of its bytes) in the
   older formats. On POWER9, the code paths for the older formats are
   about 1.6x slower per weight than the K-format paths. We proved this
   by re-compressing BOTH models into each single format and re-timing:
   the 750M model re-stored in the old format drops from 6.4 to 3.9
   tokens/s. Even at the same format the 360M model is ~1.35x slower per
   weight (its narrow 960-wide layers carry more overhead). Two
   takeaways: when choosing models for POWER, prefer internal widths
   divisible by 256; and speeding up the old formats' POWER9 code is a
   worthwhile upstream contribution (faster paths already exist for
   x86/ARM but not POWER).
8. **The real experiment fits its budget.** This practice pass (260
   measurements) took 8.3 hours; scaling by the registered plan stays
   inside the estimated 60-80 hours.

## Problems the practice run caught — the point of a practice run

Three defects were found before any real data existed, each fixed and
signed off by the matching independent expert reviewer, and each recorded
as a formal amendment to the registered protocol:

1. **A maths bug in the warm-up detector** (statistician sign-off): the
   registered formula for spotting when the browser side has warmed up
   could never trigger on realistic data — its threshold was on the wrong
   scale, off by orders of magnitude. Caught by the rule that all analysis
   code must be written and committed before any data is collected.
2. **A memory-placement check that could never pass** (experimentalist
   sign-off): the rig requires 99% of a run's memory on the declared RAM
   bank, but the check also counted the benchmark program's own code
   (~10 MB the operating system had cached on the other socket, and which
   no setting can move). Every run on the second socket failed. The check
   now counts only the memory that matters — the model and the run's
   working memory — and reports the excluded code pages separately.
3. **The swap file quietly undid our memory placement** (experimentalist
   sign-off): the machine turned out to have a 128 GiB swap file, and
   under memory pressure the OS moved our carefully-placed model copies
   out and back to the wrong socket. Swap is now off for the campaign
   (restored afterwards), and placement is re-verified before every round,
   with automatic re-staging.

The discard counts tell the same story honestly: 76 discards in the main
pass and 64 in the first re-run — all from these two placement defects —
then **zero** discards in the final re-run.

## Machine state and next steps

Atlas currently holds the campaign settings (swap off, interrupt handling
and background services corralled onto one spare core, kernel memory
options set); `harness/atlas_restore.sh` reverts everything. Next, per
the registered order: calibration runs (measuring a configuration against
itself to prove the pipeline reports "no difference" when there is none),
a memory-bandwidth ceiling measurement (STREAM), and then the real
campaign.

## Where everything lives

- Binding rules: `docs/research/benchmark-protocol.md` (with amendment
  history in place)
- Raw data: `docs/research/data/pilot/` (one JSON line per repetition;
  discard log; environment record; format-speed test)
- Full evidence (placement snapshots, frequency samples, raw benchmark
  output): archive on atlas at
  `/mnt/verus/openpower-tools/benchmarks/pilot-2026-09-01.tar.zst`,
  SHA-256 in `docs/research/manifest.json`

## The numbers

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


## Glossary

- **token** — a word fragment, roughly 3/4 of an English word.
- **socket / node** — one of the two processor chips and its attached RAM
  bank. "node 0" and "node 8" are the hardware names of the two banks.
- **core / hardware thread (SMT)** — each of the 18 cores per socket can
  run 1, 2 or 4 instruction streams ("threads").
- **pinning** — forcing a program onto exact cores and an exact RAM bank
  so runs are comparable; the rig verifies this from kernel records.
- **NUMA / memory locality** — RAM attached to the other socket is
  reachable but slower; placement therefore matters and is checked.
- **quantisation format** — the compressed storage form of model weights;
  the "K" family compresses in 256-number blocks, the older family in
  32-number blocks, and each format has its own speed on each CPU.
- **round** — one time-separated pass over all configurations in shuffled
  order; five rounds per configuration.
- **median** — the middle value; **spread** — variation as a percentage
  of the average (see "How to read the numbers").
