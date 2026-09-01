# Pre-registered benchmark protocol: op-ask inference on POWER9 and in-browser wasm

STATUS: DRAFT UNDER CROSS-FUNCTIONAL REVIEW (2026-09-02) — data collection
does not begin until review findings are resolved and this line is replaced
by the registration line in a subsequent commit.

Drafted 2026-09-02, before any instrumented run. Synthesised from four
expert research reports (Bayesian benchmarking methodology; native llama.cpp
on POWER9; wllama/headless-Chromium practice; WebLLM/provider landscape),
each fact web- or source-verified on 2026-09-01. This document is the
commitment: configurations, thresholds, ROPEs, sample sizes, priors and
decision sentences are fixed here; any deviation must be recorded as an
amendment in git history, and extensions of data collection follow the
precision-only rule below.

## 1. Claims to be decided

The experiment decides these pre-registered sentences and nothing else:

- D1 (browser slow mode): ships iff P(typical single-thread tg tok/s >= 4) >= 0.95
  AND P(a fresh browser session achieves >= 3 tok/s) >= 0.90, for at least one
  approved model at Q4_K_M, k=2 RAG chunks, on the POWER9 desktop-proxy cell
  (4-core pinned) — with the 94% HDI of typical tg entirely above 3.5.
- D2 (native companion promise): the companion app advertises the largest
  round number X* such that P(fresh-run tg >= X*) >= 0.90 at the t=4 and t=8
  desktop-proxy cells (per model; promises quoted per model).
- D3 (GGML_P9_AMO patch adoption): adopt iff P(tg ratio on/off > 1) >= 0.95 in
  at least one t >= 16 cell AND no t <= 8 cell shows P(ratio < 0.98) >= 0.50.
  ROPE for "no practical change" is |log ratio| < log(1.02).
- D4 (RAG TTFT budget): the op-ask UI shows a progress affordance iff the
  posterior median TTFT at k=4 chunks exceeds 2.0 s in the shipping tier.

Thresholds and ROPE widths are engineering judgments fixed now; they may not
be revised after data are seen.

## 2. Factors and cells

Native (atlas, single-socket pinned unless stated; models by SHA256:
SmolLM2-360M-Instruct Q4_K_M bartowski; Qwen3-0.6B Q4_K_M unsloth + official
Q8_0; Qwen3-Embedding-0.6B Q8_0):

- threads: 1, 2, 4, 8, 12, 18 (one per core, spread across core PAIRS —
  CPUs every-8 apart — because L2/L3 are per core-pair)
- SMT within-core: 1, 2, 4 threads/core at the 4- and 8-core cells
- placement: node 0 primary; one node-8 sanity config; one cross-socket
  `--numa distribute` secondary config
- tests: pp512+tg128 baseline; prefill sweep p in {1024, 2048, 4096};
  decode-at-depth d in {0, 1024, 2048, 4096}; ubatch {256, 512, 1024} at
  pp2048; batched-bench TTFT grid; embedding via `llama-bench -embd 1` and a
  `llama-embedding` corpus run; GBNF on/off via llama-completion A/B
  (sampler time reported separately; llama-bench cannot see sampling)
- AMO: GGML_P9_AMO {off, on} as a secondary experiment at t in {8, 16, 32},
  identical binary except the three chunk-claim sites (threadpool chunk_add,
  mul_mat claim, graph_compute_thread) switched to `amo_lwat_add`

Browser (same host + one x86 control; wllama 3.6.1 pinned, local files only):

- tier: default flags; `--liftoff-only`; `--no-liftoff --no-wasm-lazy-compilation`
- model: stories15M-q4_0 (smoke); SmolLM2-360M / 1B-class Q4_0 and Q4_K_M
  (never IQ quants — no wasm SIMD kernels)
- k injected chunks: 0, 2, 4, 8 (n_ctx sized to fit; max_tokens=64, temp=0,
  fixed seed, cache_prompt=false every rep)
- stages per repetition: query embed (timed around createEmbedding, tokens
  recorded); vector scan (K-batched windows >= 100 ms); generation with
  engine timings (prompt_ms / predicted_ms separately) + streamed TTFT

## 3. Environment controls (all mandatory, recorded per run)

Native: governor `performance`; `kernel.numa_balancing=0`; model files
staged into /dev/shm BY a copy executed under `numactl --membind=<target>` —
tmpfs pages are allocated at write time, so the copy's binding decides
weight-page placement; re-stage on any node switch. Every run under
`numactl --membind + --physcpubind` with `--cpu-strict 1`; `--delay 3`;
frequency + temperature (`cpupower`, `sensors`) logged before/after; loadavg
and arcstat recorded for the manifest only — ZFS ARC is reclaimable cache
that yields to memory demand (owner's standing note): it is NOT treated as
pressure, NOT capped, and free-memory readings are never interpreted without
it; tmpfs staging exists for I/O determinism, not ARC avoidance. No other
tenants.

NUMA attestation gate (hard, per run): `numastat -p <pid>` and
`/proc/<pid>/numa_maps` must show >= 99% of resident pages on the bound node,
and every compute thread must be observed on its pinned CPUs; a run failing
attestation is discarded with cause recorded and the condition re-run. The
node-numbering trap is explicit: on atlas node 8 = CPUs 0-71 (socket 0),
node 0 = CPUs 72-143 (socket 1); SMT siblings are consecutive CPU numbers,
so distinct cores are every 4th CPU and distinct core-pairs every 8th. Build gate: cmake resolves `-mcpu=power9`; objdump shows VSX
(lxv/vmsummbm/xvmaddasp >> 0) and ZERO `*ger` MMA mnemonics; system_info
`VSX = 1`; fixed-seed 64-token output smoke matches reference.

Browser: Chromium `--headless=new` with the throttling-disable flag set,
`--disable-gpu`, fresh `--user-data-dir` per invocation, taskset+numactl
pinning of the whole browser; bench server sends NO COOP/COEP (parity with
Pages; crossOriginIsolated===false asserted in-page); `setCompat(null)` and
build attestation logged (libllama version, isMultithread=false, n_threads=1,
n_gpu_layers=0); timer coarsening (100 microseconds) respected by timing only
windows >= 100 ms.

## 4. Design, sample size, stopping

- RMIT interleaving: one run per configuration per round, order re-randomised
  each round; >= 3 blocks at different times of day; block recorded.
- A/A calibration of the entire pipeline (two aliases of one config) must
  yield a ratio posterior inside the ROPE before any real comparison is run.
- Pilot: 5 runs per cell to estimate between-run CV and dimension n.
- Fixed n from pilot, defaults: native 10 runs x (-r 10) per cell; browser
  25 invocations x (>= 10 recorded iterations, warm-up discarded only on
  changepoint evidence, never by convention).
- Extension rule (pre-registered, precision-only): if the 94% HDI of mu is
  wider than +/-2% (log scale), collect 10 more runs; maximum 3 extensions;
  never extend because a decision probability is close to its threshold.
  All collected data are reported.

## 5. Statistical model and reporting

Per metric (pp and tg separately; never pooled, never ratio-averaged):
hierarchical Student-t on log(tok/s), run-level random effect (non-centred),
priors: mu ~ Normal(log guess, 1.0); tau ~ HalfNormal(0.10); sigma_w ~
HalfNormal(0.05); nu ~ Gamma(2, 0.1) + 1. Inference: PyMC + ArviZ under
`uv run`, 4 chains, target_accept 0.95; gates R-hat <= 1.01, adequate ESS,
zero divergences; prior- and posterior-predictive checks. Cross-checks:
conjugate closed-form on run means (scipy t.sf) and BCa bootstrap; material
disagreement blocks publication.

Published per cell: n runs x reps; posterior median tok/s; 94% HDI (log
scale, exponentiated, labelled); P(>= X) for each registered threshold,
typical AND predictive, labelled; between-run and within-run spread (%);
posterior nu; diagnostics; environment manifest; links to raw JSONL, script,
seed. Comparisons: ratio median, 94% HDI, P(ratio > 1), ROPE verdict
(reject / accept-equivalence / undecided). Raw distributions plotted beside
posteriors. STREAM-triad roofline per node reported, with tg quoted as % of
roofline (the number that transfers to Talos II desktops); desktop promises
come only from t=4/t=8 pinned cells with the memory-channel caveat stated.

## 6. Sanity bands (pre-registered expectation checks)

- native-vs-wasm single-thread ratio on POWER9: expected 2-4x (explicit
  bounds checks + Memory64, offset by 128-bit vector parity); outside
  ~1.2-5x => audit the setup before believing the number.
- prefill per-token vs decode per-token, native: expect roughly 5x.
- Anomaly carried in from the smoke run, to be resolved not hand-waved:
  Qwen3-0.6B (752M) out-ran SmolLM2-360M at every thread count.

## 7. Provenance

Expert reports (session transcripts, 2026-09-01) with all primary sources:
Bayesian methodology (Hoefler & Belli SC'15; Kalibera & Jones ISMM'13;
Kruschke 2013/2018; Furia et al. TSE'21/TOSEM'22; Barrett et al. OOPSLA'17;
Abedi & Brecht ICPE'17; de Heide & Gruenwald 2021; Kazin 2026); native
llama.cpp/POWER9 (llama-bench source @ 0eadefe; ggml powerpc quants.c;
llamafile sgemm MMA gating; atlas live probes incl. 238-vs-38-instruction
-mcpu proof); browser wasm (wllama 3.6.1 source; V8 ppc64 sources: Liftoff,
unconditional SIMD128, no wasm trap handler on ppc64; Chrome headless flags;
timer coarsening); AMO study (POWER9 UM v2.1 section 10.8 pp.191-192; Power
ISA 3.0B section 4.5 pp.857-862; gcc amo.h lwat codegen verified on atlas).
