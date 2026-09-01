# Pre-registered benchmark protocol: op-ask inference on POWER9 and in-browser wasm

STATUS: REVISION 2 (2026-09-02) — round-1 panel findings (statistician,
systems experimentalist, browser/VM engineer: three REQUEST REVISION
verdicts) are addressed in this text. Awaiting panel sign-off; the
registration line replaces this status in a subsequent commit, and data
collection begins only after that commit. Artifact digests live in
docs/research/manifest.json (committed alongside; amendments to it are
committed BEFORE first use of each artifact, never after).

Drafted 2026-09-02 from four expert research reports (Bayesian methodology;
native llama.cpp/POWER9; wllama/headless Chromium; WebLLM landscape), each
fact source-verified 2026-09-01. Revision 2 incorporates the round-1 reviews
in full; deviations during the campaign require amendment commits.

## 0. Definitions (all decision sentences resolve against these)

- Generative model set G (byte-identical files native and browser):
  - m1 = SmolLM2-360M-Instruct Q4_K_M, sha256
    2fa3f013dcdd7b99f9b237717fa0b12d75bbb89984cc1274be1471a465bac9c2
  - m2 = Qwen3-0.6B Q4_K_M, sha256
    9acfc1e001311f34b4252001b626f2e466d592a42065f66571bff3790d4e1b14
  Qwen3-0.6B Q8_0 (official artifact) is exploratory only, non-decisional.
- Embedding model E = Qwen3-Embedding-0.6B Q8_0 (official repo); digest to
  manifest before first use. Smoke model: stories15M-q4_0 (digest likewise).
- Browser build B = the wllama 3.6.1 COMPAT build (ASYNCIFY, no Memory64),
  vendored locally and loaded via explicit WllamaCompat paths on BOTH
  architectures — V8's ppc64 port has no JSPI (verified in builtins-ppc.cc),
  so the main build cannot run there; identical binaries per cell are
  mandatory. Main-build cells are x86-only exploratory, non-decisional.
- Shipping tier := default V8 flags on build B (fixed now, resolving the
  tier-selection circularity).
- llama.cpp commit: 0eadefebd3f8f92a86d634a0e5b8fffc9dc792c0 (native and the
  wllama-pinned build recorded in manifest).
- "typical" := the posterior of mu (run-level location, log scale).
- "fresh run" / "fresh session" := the posterior predictive of a NEW
  RUN-LEVEL MEAN (mu + b_new, integrating block and run effects tau_b,
  tau_r; NOT a single new observation). Browser mapping: "fresh session" =
  the recorded steady-state iterations of a fresh browser invocation (fresh
  --user-data-dir; never a reload — V8's in-process module cache would leak
  tiered code). The decisional estimand is WARM steady state; the cold
  first-response (pre-changepoint) is reported descriptively per cell
  (median cold-vs-warm delta) but is not decisional.
- TTFT := engine prompt_ms + first predicted token's per-token ms (engine
  definition; comparable native vs browser). Streamed first-chunk arrival
  and user-perceived wait (embed + scan + streamed TTFT) are reported
  alongside, labelled, non-decisional.
- Multiplicity posture: every D-sentence is evaluated and reported PER MODEL
  in G, for all of G — no existential quantification over hidden cells; the
  registered cells for each sentence are named in section 1. Cells are fit
  jointly where paired (D3) and stratified otherwise; because stratified
  fits do not shrink, the per-sentence cell lists here ARE the multiplicity
  control: nothing outside them may be quoted as a decision.

## 1. Decision sentences

- D1 (per model m in G): browser slow mode ships for m iff, in cell BPX
  (browser proxy, shipping tier, k=2), P(typical tg >= 4 tok/s) >= 0.95 AND
  P(fresh-session tg >= 3 tok/s) >= 0.90.
- D2 (per model m in G): the native companion quotes X*(m) = the largest
  integer X such that min over cells {N4, N8} of
  P(fresh-run tg >= X) >= 0.90. Promises are quoted raw, with achieved MHz
  published beside them (frequency-annotation choice registered here).
- D3 (AMO adoption; model m1, joint paired model with shared block effects,
  runtime-toggle single binary): adopt iff
  P(log ratio(on/off) > log 1.02) >= 0.95 for tg in ANY of cells
  {A16, A32, AX36} AND for BOTH guards {A4, A8}:
  P(log ratio < log 0.98) < 0.50 for tg AND for pp.
  The superiority bound equals the ROPE bound, so adoption cannot trigger
  inside "practically nil". Per-cell probabilities and the joint fit are all
  published.
- D4 (per model m in G): the op-ask UI shows a progress affordance iff
  P(fresh-session TTFT at k=4, cell BPX > 2.0 s) >= 0.50. TTFT is modelled
  with the same hierarchical Student-t on log seconds as every other metric.

## 2. Cells — executable definitions

Atlas topology (verified): node 8 = CPUs 0-71 (socket 0), node 0 = CPUs
72-143 (socket 1); core k = CPUs 4k..4k+3 (SMT4 siblings consecutive);
L2/L3 shared per core PAIR (device-tree l2-cache phandles — sysfs shows L1
only, so pair attestation uses the device tree).

Native cells (node 0 unless stated; every-8 = distinct core pairs):

| cell | CPUs (--physcpubind) | t | note |
|---|---|---|---|
| N1 | 72 | 1 | baseline rows only |
| N2 | 72,80 | 2 | baseline rows only |
| N4 | 72,80,88,96 | 4 | full rows; D2 cell |
| N8 | 72,80,88,96,104,112,120,128 | 8 | full rows; D2 cell |
| N12 | N8 + 136,76,84,92 | 12 | pairs 0-2 doubled (stated) |
| N18 | 72,76,80,...,140 (every 4) | 18 | full rows |
| P4 | 72,76,80,84 | 4 | packed pairs (desktop lower bound) |
| P8 | 72,76,80,84,88,92,96,100 | 8 | packed pairs |
| S4x2 | 72,73,80,81,88,89,96,97 | 8 | 4 cores x SMT2 (vs N8) |
| S4x4 | 72,73,74,75,...,96..99 | 16 | 4 cores x SMT4 |
| S8x2 | N8 cores x SMT2 | 16 | vs S4x4: SMT de-conflation |
| N8b | 0,8,16,24,32,40,48,56 (node 8) | 8 | socket sanity, baseline rows |
| AX | --interleave=0,8, distribute | 36 | cross-socket; gate 45-55%/node |

AMO cells (model m1, baseline rows pp512+tg128 only): A4, A8, A16, A32 on
the N4/N8/S8x2/(N18+SMT2 = 32-thread list 72,73,76,77,...) lists, plus AX36
= the AX cell; each run twice with the runtime toggle off/on inside the same
block round (paired).

Browser cells: BPX = Chromium pinned via taskset to CPUs 72,80,88,96 with
membind node 0 (browser-process pinning; the wasm engine itself is
single-threaded — n_threads=1 asserted). x86 control host: named in the
manifest before browser runs; role = qualitative context only, never
decisional.

Row sets (budget-pruned per review; registered): baseline = pp512 + tg128.
Full rows = baseline + pp{1024,2048,4096} + d{1024,2048,4096} + ubatch
{256,1024} at pp2048 (ubatch 512 is the pp2048 default row — not
duplicated). Full rows run ONLY at N4, N8, N18; all other native cells run
baseline only. Deep sweep on m1; m2 runs baseline everywhere plus full rows
at N4 and N8 only. r=10 for rows with per-rep < 60 s, r=3 otherwise
(registered by row class, not chosen ad hoc). Estimated budget: ~60-80 h
native + ~20-25 h browser, plus pilot; acceptable and survivable without
deviation.

Browser factors: tier in {shipping (default flags), liftoff =
"--liftoff-only --no-wasm-lazy-compilation", eager = "--no-liftoff
--no-wasm-lazy-compilation"} — all passed via --js-flags and PROVEN applied
by capturing the renderer's /proc/<pid>/cmdline; k in {0,2,4,8} with ONE
n_ctx per model across all k (sized for k=8; KV size must not confound k);
generation: greedy (temp=0, seed inert and stated), max_tokens=64,
cache_prompt=false, n_gpu_layers=0. Default-tier cells: >= 20 recorded
iterations (changepoint power); other tiers >= 10. 25 invocations per cell.
Embedder/generator procedure (registered): two wllama instances — embedder
loaded, used for the query, fully unloaded BEFORE the generator loads; load
times reported separately.

## 3. Environment controls (mandatory; recorded per run)

Native:
- Governor performance; in-run frequency sampling at ~1 Hz of
  scaling_cur_freq on the pinned CPUs (before/after snapshots are inert —
  idle cores read max boost); achieved MHz published per cell.
- kernel.numa_balancing=0 verified at every run start, restored after the
  campaign (it is 1 in normal operation). Kernel version + cmdline in
  manifest. THP set to madvise for the campaign (atlas default
  always/shmem-never would give STREAM 2 MB THP while tmpfs weights never
  get it — a systematic roofline-vs-workload TLB asymmetry).
- irqbalance stopped; default and per-IRQ smp_affinity moved to the
  non-measurement socket; system slices corralled via systemd AllowedCPUs
  away from measurement CPUs AND their SMT siblings; sibling idleness
  attested via /proc/stat busy deltas during 1-thread-per-core cells.
- Model staging: files copied into /dev/shm BY a copy running under
  numactl --membind=<target node> (tmpfs pages are placed at write time);
  re-staged on any node switch; staging attested once with numastat.
- ZFS ARC is reclaimable cache (owner's standing note): recorded for the
  manifest, never treated as pressure, never capped; expect the first
  cold-node membind allocation to evict ARC and run long — the changepoint
  rule absorbs it; do not misread it as an anomaly.
- llama-bench: -o json (raw samples_ns kept), --delay 3, warmup on
  (llama-bench's untimed warmup makes native reps steady-state by tool
  construction — stated for symmetry with the browser evidence rule),
  threads via -C <mask> --cpu-strict 1 matching the cell's CPU list exactly
  (numactl provides membind + the outer affinity; the ggml mask must equal
  the registered list, and both are recorded).

NUMA attestation gate (hard, per run): numastat -p and /proc/<pid>/numa_maps
snapshotted (a) after model load + first measured rep and (b) at run end;
>= 99% of resident pages on the bound node (AX cell: 45-55% per node);
thread placement attested by sampling /proc/<pid>/task/*/stat field 39 at
>= 5 steady-state points — every sample within the cell's CPU list — and
per-thread Cpus_allowed_list equal to the registered list. numa_maps
snapshots are kept in the artifacts as ground truth. A run failing any
attestation is discarded with cause recorded and re-run.

AMO attestation (single binary, runtime toggle): the binary carries both
paths behind a run-time switch (predicted branch per claim site); objdump
must show lwat at exactly the three chunk-claim sites and nowhere else;
fixed-seed 64-token output identical toggle-on vs toggle-off; the three
counters verified to occupy private 128-byte lines (struct layout documented
in the manifest). The 1.9%-median-regression edge of the D3 guard is
accepted deliberately: the guard exists to catch clear regressions, and the
superiority requirement already gates adoption.

Browser:
- Chromium flags (exact, enumerated): --headless=new
  --disable-background-timer-throttling --disable-backgrounding-occluded-windows
  --disable-renderer-backgrounding --disable-features=IntensiveWakeUpThrottling
  --disable-gpu --disable-extensions --no-first-run --mute-audio
  --user-data-dir=<fresh per invocation>
  --host-resolver-rules="MAP * ~NOTFOUND, EXCLUDE localhost"
  --js-flags=<per tier>. Driver: plain process launch + POST-back harness
  (no Puppeteer/Playwright — no ppc64le builds); bench server code and
  response headers published with artifacts; server sends NO COOP/COEP
  (crossOriginIsolated===false asserted in-page).
- Build attestation (evidence, not config echo): SHA256 + URL of every
  .wasm/.js actually fetched (network log), zero non-localhost requests
  asserted, per-run probes logged (typeof WebAssembly.Suspending, Memory64
  probe, SIMD validate probe, navigator.gpu + requestAdapter() result),
  wllama attested via explicit WllamaCompat local paths (vendored compat
  build, hashes in manifest), libllama version, isMultithread()===false,
  getNumThreads()===1. Chromium package NEVR + V8 version pinned in the
  manifest (Fedora ppc64le container image pinned; V8 majors matched to the
  x86 control where possible, skew recorded). Models loaded from Blob/File
  (skips OPFS churn); RAG chunk texts and the GBNF grammar file pinned by
  hash.
- Timer discipline: performance.now coarsened to 100 us without isolation —
  every measured window must be >= 100 ms; embedding and any fast stage are
  auto-batched (K repeats per window, K sized at runtime) exactly like the
  vector-scan stage; engine prompt_ms/predicted_ms used as stage sums only,
  never per-token latencies.

## 4. Design, sample size, stopping

- Blocks: 5-6 time-separated blocks (not 3 — block variance is weakly
  identified otherwise); RMIT interleaving within blocks: one run per cell
  per round, order re-randomised each round; AMO on/off paired within the
  same round.
- A/A calibration, per pipeline (native AND browser), at full registered
  per-cell n, before any real comparison: the 94% HDI of the A/A log-ratio
  must lie wholly within +/- log 1.02; the A/A fit's tau_b, tau_r, sigma_w,
  nu and posterior-predictive coverage are reported against pilot
  expectations; a failed A/A is diagnosed, documented, and re-run after the
  cause is fixed. One-time absolute-scale instrument validation: engine
  tok/s vs independent wall clock over >= 100 ms windows must agree within
  1% (A/A ratios cannot validate the absolute scale D1/D4 consume).
- Pilot: 5 runs per cell, EXCLUDED from decision posteriors (dimensioning
  only: between-run/between-block CV estimates).
- Base n: native 10 runs x (r per row class) per cell; browser 25
  invocations per cell. Extension rule (precision-only, decision-blind):
  if the 94% HDI of mu for a decision metric in a decision cell is wider
  than +/- 2% (half-width, log scale), extend that cell by 5 native runs or
  8 browser invocations; maximum 3 extensions; never extend on the value of
  any decision probability. All data collected are reported. The design is
  base-n plus a capped, decision-blind precision-extension schedule (not
  fixed-n; named accurately).
- Warm-up / steady state (browser): per invocation, PELT changepoint
  segmentation on log per-iteration times (ruptures, cost l2, min_size=3,
  BIC-style penalty fixed in the analysis script); steady state = final
  segment, required >= 5 iterations with no monotone trend; otherwise the
  invocation is marked no-steady-state, recorded, and re-run. Only
  pre-final-segment iterations are discarded. Native needs no equivalent
  (untimed warmup by tool construction).

## 5. Statistical model, checks, reporting

Per metric (pp, tg, TTFT, embed-ms/token, scan-ms — each separately; never
pooled, never ratio-averaged): hierarchical Student-t on log(value) with
block and run levels (non-centred):

  y[c,b,r,i] ~ StudentT(nu, mu[c] + u[c,b] + v[c,b,r], sigma_w[c])
  u ~ Normal(0, tau_b);  v ~ Normal(0, tau_r)

Priors: mu ~ Normal(log guess, 1.0); tau_b, tau_r ~ HalfNormal(0.10);
sigma_w ~ HalfNormal(0.05); nu ~ 1 + Gamma(2, 0.1) — Gamma parameterised by
RATE (PyMC beta; scipy users beware scale). Mandatory prior-sensitivity
suite: refits with tau_* ~ HalfNormal(0.2), sigma_w ~ HalfNormal(0.1),
mu sd 2.0, and nu ~ 1 + Exponential(1/29); every D-verdict must be invariant
across the suite; any flip => the sentence is undecided, both fits are
reported, and the precision-extension rule applies.

Inference: PyMC + ArviZ under uv run, 4 chains, target_accept 0.95,
random_seed recorded per fit. Gates (numeric): R-hat <= 1.01; bulk ESS >=
1000 and tail ESS >= 400 per parameter; MCSE of every decision probability
<= 0.005; zero divergences; prior- and posterior-predictive checks with
registered criteria (posterior-predictive p-values for the median and IQR
statistics within [0.05, 0.95], else remodel and report). Cross-checks:
conjugate closed-form on run means and BCa bootstrap of run means;
"material disagreement" := any D-verdict flip OR a median shift exceeding
half the ROPE width — disagreement blocks publication until resolved
(bootstrap noise at n=10 is expected and does not alone constitute a flip).

Published per cell: n (blocks x runs x reps, discards + causes); posterior
median; 94% HDI (log scale, exponentiated, labelled — equal-tailed interval
also given where a natural-scale threshold is quoted); P(>= X) for each
registered threshold, typical AND fresh-run, labelled; tau_b, tau_r,
sigma_w as multiplicative %; posterior nu; diagnostics; achieved MHz;
environment manifest; raw JSONL + scripts + seeds. Comparisons: ratio
median, 94% HDI, P(ratio > 1), P(beyond ROPE bound), verdict trichotomy.
Raw distributions plotted beside posteriors.

STREAM roofline (registered configuration): McCalpin stream.c 5.10, GCC 13.3
-O3 -mcpu=power9 -fopenmp, STREAM_ARRAY_SIZE = 80,000,000 (>= 4x socket
LLC), NTIMES = 20, best-of per STREAM convention with all iterations
recorded, run under the cell's numactl with OMP_PROC_BIND=true and
OMP_PLACES = the cell's exact CPU list. Published: full-socket t=18 triad
per node (the roofline), matched-t triads for t in {4, 8} (the denominators
for D2 quotes — % of full-socket roofline is NOT quoted at low t), and an
interleaved triad if the AX cell is ever quoted as a percentage. tg
efficiency = model-bytes x tok/s / matched-t triad.

## 6. Sanity bands and carried anomalies

- native-vs-wasm single-thread ratio, ppc64le, SAME Q4_K_M file, compat
  build, shipping (tiered-up) tier only: expected 2-4x; audit outside
  ~1.2-5x. Liftoff-only cells are exempt (legitimately >5x on ppc64le: no
  trap handler, explicit bounds checks, baseline codegen). Named false
  pushers: compat/main build mismatch, pre-tier-up iterations included,
  --js-flags not applied (check cmdline capture), quant mismatch, V8 major
  skew, native baseline unpinned.
- prefill-vs-decode per-token, native: expect ~5x.
- Carried anomaly (must be resolved, not hand-waved): Qwen3-0.6B (752M)
  out-ran SmolLM2-360M at every thread count in the unregistered smoke run.

## 7. Provenance

Round-1 panel reviews (statistician; systems experimentalist with live
atlas verification incl. device-tree cache phandles, WOF range, irqbalance
state; browser/VM engineer with V8-source verification incl. the absence of
JSPI on ppc64) are preserved in session transcripts; their findings are
implemented above. Underlying research reports and primary sources as in
revision 1: Hoefler & Belli SC'15; Kalibera & Jones ISMM'13; Kruschke
2013/2018; Furia et al.; Barrett et al. OOPSLA'17; Abedi & Brecht ICPE'17;
Kazin 2026; llama.cpp @ 0eadefe sources; wllama 3.6.1 sources; V8 trunk
sources; POWER9 UM v2.1 sections 9/10.8; Power ISA 3.0B section 4.5; gcc
amo.h lwat codegen verified on atlas (page citations to be re-verified at
publication).
