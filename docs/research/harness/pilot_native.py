#!/usr/bin/env python3
# /// script
# requires-python = ">=3.11"
# dependencies = []
# ///
"""Native pilot runner for benchmark-protocol.md (REGISTERED, rev 3 as amended).

Runs ON atlas under uv-managed python (uv run pilot_native.py ...);
stdlib-only dependencies. Implements the registered native cells
(section 2), environment controls and attestations (section 3), and the
pilot design (section 4: 5 runs per cell, dimensioning only, EXCLUDED from
decision posteriors).

Pilot layout: 5 time-separated rounds; within each round the (cell, model)
order is re-randomised (RMIT); block = run = round index in the output
JSONL (one run per cell per round), matching analyze.py's input schema:
    {"cell": C, "block": k, "run": k, "metric": "pp512"|"tg128", "value": tok_s}
Rows: baseline only (pp512 + tg128), per the registered pruning (full rows
are a campaign matter; the pilot dimensions CV).

Repetition policy (registered by row class, not ad hoc): r=10 for rows with
per-rep < 60 s, r=3 otherwise. Applied from the smoke-run rates (m1 t=1
pp 6.3 tok/s -> pp512 ~81 s/rep): pp512 gets r=3 at t=1, else r=10; tg128
always r=10. Deterministic, encoded below.

Evidence per invocation (evidence/<model>/<cell>/round<k>/):
  bench.json          raw llama-bench -o json output
  freq.csv            ~1 Hz scaling_cur_freq of the cell's CPUs
  stat39.csv          sampled /proc/<pid>/task/*/stat field 39 (CPU placement)
  numastat.{a,b}      numastat -p at RSS-stable and at end
  numa_maps.{a,b}     /proc/<pid>/numa_maps snapshots (ground truth)
  siblings.json       /proc/stat busy deltas for SMT siblings (1 t/core cells)
  attest.json         gate results + cmdline + Cpus_allowed_list

Gates (hard): >=99% resident pages on the bound node (AX: 45-55% per node);
every stat39 sample within the cell's CPU list; per-thread
Cpus_allowed_list equal to the registered list. A failing run is discarded
with cause recorded and re-run once.

Sacrificial core (operational note, recorded): IRQs + system.slice are
corralled to CPUs 60-63 (node 8 core 15) by atlas_setup.sh. Only the AX
cell touches CPU 60; its runs record a sibling-idleness exemption for that
core. The pilot quantifies the cost; an amendment may shrink AX to 35
cores if it matters.
"""

import argparse
import hashlib
import json
import os
import random
import shutil
import subprocess
import sys
import threading
import time
from pathlib import Path

BENCH = Path.home() / "op-ask-spike/llama.cpp/build/bin/llama-bench"
BENCH_COMMIT = "0eadefe"
MODELS = {
    "m1": {
        "src": Path.home() / "op-ask-spike/smollm2-360m-q4km.gguf",
        "sha256": "2fa3f013dcdd7b99f9b237717fa0b12d75bbb89984cc1274be1471a465bac9c2",
    },
    "m2": {
        "src": Path.home() / "op-ask-spike/qwen3-0.6b-q4km.gguf",
        "sha256": "9acfc1e001311f34b4252001b626f2e466d592a42065f66571bff3790d4e1b14",
    },
}
SHM = Path("/dev/shm/opb")
SACRIFICIAL = {60, 61, 62, 63}

def _every(start, stop, step):
    return list(range(start, stop + 1, step))

N8_CPUS = [72, 80, 88, 96, 104, 112, 120, 128]
CELLS = {
    # name: (cpus, t, placement) — placement: 0 / 8 (membind node) or "inter"
    "N1": ([72], 1, 0),
    "N2": ([72, 80], 2, 0),
    "N4": ([72, 80, 88, 96], 4, 0),
    "N8": (N8_CPUS, 8, 0),
    "N12": (N8_CPUS + [136, 76, 84, 92], 12, 0),
    "N18": (_every(72, 140, 4), 18, 0),
    "P4": ([72, 76, 80, 84], 4, 0),
    "P8": ([72, 76, 80, 84, 88, 92, 96, 100], 8, 0),
    "S4x2": ([72, 73, 80, 81, 88, 89, 96, 97], 8, 0),
    "S4x4": ([72, 73, 74, 75, 80, 81, 82, 83, 88, 89, 90, 91, 96, 97, 98, 99], 16, 0),
    "S8x2": (sorted(c for base in N8_CPUS for c in (base, base + 1)), 16, 0),
    "N8b": ([0, 8, 16, 24, 32, 40, 48, 56], 8, 8),
    "AX": (_every(0, 68, 4) + _every(72, 140, 4), 36, "inter"),
}

def one_thread_per_core(cpus):
    return all(c % 4 == 0 for c in cpus)

def reps_for(row, t):
    if row == "pp512":
        return 3 if t == 1 else 10
    return 10  # tg128

def cpu_mask(cpus):
    return f"0x{sum(1 << c for c in cpus):x}"

def sha256_file(path):
    h = hashlib.sha256()
    with open(path, "rb") as fh:
        for chunk in iter(lambda: fh.read(1 << 20), b""):
            h.update(chunk)
    return h.hexdigest()

def run_out(argv, **kw):
    return subprocess.run(argv, capture_output=True, text=True, **kw)

def log(msg):
    print(f"[{time.strftime('%H:%M:%S')}] {msg}", flush=True)

# ---------- staging ----------

def staging_path(model, placement):
    sub = {0: "node0", 8: "node8", "inter": "inter"}[placement]
    return SHM / sub / f"{model}.gguf"

def numactl_prefix(placement, cpus=None):
    if placement == "inter":
        pre = ["numactl", "--interleave=0,8"]
    else:
        pre = ["numactl", f"--membind={placement}"]
    if cpus is not None:
        pre.append("--physcpubind=" + ",".join(map(str, cpus)))
    return pre

def map_locality(path):
    """Map the file, touch every page, report this process's page placement
    for that mapping from /proc/self/numa_maps (tmpfs pages ARE the file)."""
    import mmap
    with open(path, "rb") as fh:
        mm = mmap.mmap(fh.fileno(), 0, prot=mmap.PROT_READ)
        step = 65536  # 64K pages on atlas
        total = 0
        for off in range(0, len(mm), step):
            total += mm[off]
        base = None
        counts = {}
        for line in open("/proc/self/numa_maps"):
            if str(path) in line:
                for tok in line.split():
                    if tok.startswith("N") and "=" in tok:
                        node, pages = tok[1:].split("=")
                        counts[int(node)] = counts.get(int(node), 0) + int(pages)
        mm.close()
    return counts

def balloon_node8(gib=6):
    """Force reclaim (ARC eviction) on node 8 so strict membind and the
    interleave writer have headroom there; the balloon frees on exit.
    Rationale (measured 2026-09-01): the ~41 GiB ZFS ARC concentrates on
    node 8 (319 MB free); membind triggers eviction but --interleave
    silently falls back to node 0 (observed 94/6 on a staged copy)."""
    log(f"balloon: touching {gib} GiB under membind=8 to force ARC eviction")
    code = (f"x = bytearray({gib} << 30)\n"
            "for i in range(0, len(x), 65536):\n"
            "    x[i] = 1\n")
    subprocess.run(["numactl", "--membind=8", sys.executable, "-c", code], check=True)

INTER_CHUNK = 65536  # one 64K page on atlas

def interleave_copy(src, dst):
    """Deterministic 50/50 interleave: even 64K chunks written under
    membind=0, odd under membind=8 (tmpfs places pages at write time, per
    offset) — allocator-fallback-proof, unlike --interleave."""
    size = Path(src).stat().st_size
    with open(dst, "wb") as fh:
        fh.truncate(size)
    helper = (
        "import sys\n"
        "src, dst, parity = sys.argv[1], sys.argv[2], int(sys.argv[3])\n"
        f"CH = {INTER_CHUNK}\n"
        "s = open(src, 'rb'); d = open(dst, 'r+b')\n"
        "off = parity * CH\n"
        "while True:\n"
        "    s.seek(off)\n"
        "    buf = s.read(CH)\n"
        "    if not buf: break\n"
        "    d.seek(off)\n"
        "    d.write(buf)\n"
        "    off += 2 * CH\n"
    )
    for parity, node in ((0, 0), (1, 8)):
        subprocess.run(["numactl", f"--membind={node}", sys.executable, "-c",
                        helper, str(src), str(dst), str(parity)], check=True)

def inter_frac0(loc):
    return loc.get(0, 0) / max(sum(loc.values()), 1)

def stage_one(model, info, placement, force=False):
    dst = staging_path(model, placement)
    dst.parent.mkdir(parents=True, exist_ok=True)
    ok = dst.exists() and not force and sha256_file(dst) == info["sha256"]
    if ok and placement == "inter":
        loc = map_locality(dst)
        if not 0.48 <= inter_frac0(loc) <= 0.52:
            log(f"stage {dst}: digest OK but placement {loc} not interleaved; re-staging")
            ok = False
    if ok:
        log(f"stage {dst}: present, digest and placement OK")
        return
    if placement == "inter":
        log(f"stage {dst} <- {info['src']} (deterministic two-pass interleave)")
        dst.unlink(missing_ok=True)
        interleave_copy(info["src"], dst)
    else:
        log(f"stage {dst} <- {info['src']} under {numactl_prefix(placement)}")
        subprocess.run(numactl_prefix(placement) + ["cp", str(info["src"]), str(dst)], check=True)
    digest = sha256_file(dst)
    assert digest == info["sha256"], f"digest mismatch for {dst}: {digest}"
    loc = map_locality(dst)
    if placement == "inter":
        assert 0.48 <= inter_frac0(loc) <= 0.52, f"interleave staging off: {loc}"
    log(f"stage {dst}: digest OK, page placement {loc}")
    att = SHM / f"staging-{model}-{placement}.json"
    att.write_text(json.dumps({"path": str(dst), "sha256": digest,
                               "placement": str(placement), "pages": loc}))

def cmd_stage(_args):
    balloon_node8()
    for placement in (0, 8, "inter"):
        for model, info in MODELS.items():
            stage_one(model, info, placement)
    log("staging complete")

def verify_staging():
    for placement in (0, 8, "inter"):
        for model, info in MODELS.items():
            dst = staging_path(model, placement)
            assert dst.exists() and sha256_file(dst) == info["sha256"], \
                f"staging invalid: {dst} (run stage first)"
    log("staged digests verified")

# ---------- preflight ----------

def read1(path):
    return Path(path).read_text().strip()

def cmd_preflight(_args):
    fails = []
    govs = set()
    for p in Path("/sys/devices/system/cpu").glob("cpu*/cpufreq/scaling_governor"):
        govs.add(p.read_text().strip())
    if govs != {"performance"}:
        fails.append(f"governor: {govs}")
    if read1("/proc/sys/kernel/numa_balancing") != "0":
        fails.append("numa_balancing != 0 (run atlas_setup.sh)")
    if "[madvise]" not in read1("/sys/kernel/mm/transparent_hugepage/enabled"):
        fails.append("THP not madvise (run atlas_setup.sh)")
    irq = run_out(["systemctl", "is-active", "irqbalance"]).stdout.strip()
    if irq == "active":
        fails.append("irqbalance active (run atlas_setup.sh)")
    if not BENCH.exists():
        fails.append(f"missing {BENCH}")
    for model, info in MODELS.items():
        if sha256_file(info["src"]) != info["sha256"]:
            fails.append(f"{model} source digest mismatch")
    # mask smoke: N4 mask, tiny run, verify build commit + samples fields
    cpus, t, placement = CELLS["N4"]
    argv = (numactl_prefix(placement, cpus)
            + [str(BENCH), "-m", str(MODELS["m1"]["src"]), "-t", str(t),
               "-C", cpu_mask(cpus), "--cpu-strict", "1",
               "-p", "16", "-n", "0", "-r", "1", "-o", "json"])
    proc = run_out(argv, timeout=600)
    if proc.returncode != 0:
        fails.append(f"mask smoke failed: {proc.stderr[-400:]}")
    else:
        rows = json.loads(proc.stdout)
        commit = rows[0].get("build_commit", "")
        if not commit.startswith(BENCH_COMMIT):
            fails.append(f"bench build_commit {commit} != {BENCH_COMMIT}")
        if "samples_ns" not in rows[0] and "samples_ts" not in rows[0]:
            fails.append(f"bench json lacks samples fields: {sorted(rows[0])}")
    if fails:
        for f in fails:
            log(f"PREFLIGHT FAIL: {f}")
        sys.exit(1)
    log("preflight OK")

# ---------- attestation machinery ----------

class Watcher(threading.Thread):
    def __init__(self, pid, cpus, evdir, row):
        super().__init__(daemon=True)
        self.pid, self.cpus, self.evdir = pid, set(cpus), evdir
        self.row = row
        self.freq = []
        self.stat39 = []
        self.allowed = set()
        self.snapshot_a_done = False
        self.stop_flag = False
        self.rss_hist = []

    def snapshot(self, tag):
        try:
            (self.evdir / f"numa_maps.{self.row}.{tag}").write_text(
                Path(f"/proc/{self.pid}/numa_maps").read_text())
            ns = run_out(["numastat", "-p", str(self.pid)])
            (self.evdir / f"numastat.{self.row}.{tag}").write_text(ns.stdout)
        except (FileNotFoundError, ProcessLookupError):
            pass

    def sample_stat39(self):
        try:
            for tstat in Path(f"/proc/{self.pid}/task").iterdir():
                data = (tstat / "stat").read_text()
                fields = data[data.rindex(")") + 2:].split()
                self.stat39.append((time.time(), tstat.name, int(fields[36])))
                status = (tstat / "status").read_text()
                for line in status.splitlines():
                    if line.startswith("Cpus_allowed_list:"):
                        self.allowed.add(line.split(":", 1)[1].strip())
        except (FileNotFoundError, ProcessLookupError, ValueError):
            pass

    def run(self):
        t0 = time.time()
        ticks = 0
        while not self.stop_flag:
            now = time.time()
            try:
                for line in Path(f"/proc/{self.pid}/status").read_text().splitlines():
                    if line.startswith("VmRSS:"):
                        self.rss_hist.append(int(line.split()[1]))
            except (FileNotFoundError, ProcessLookupError):
                break
            for c in self.cpus:
                try:
                    f = read1(f"/sys/devices/system/cpu/cpu{c}/cpufreq/scaling_cur_freq")
                    self.freq.append((round(now - t0, 1), c, int(f)))
                except (FileNotFoundError, OSError):
                    pass
            if (not self.snapshot_a_done and len(self.rss_hist) >= 4
                    and self.rss_hist[-1] > 150_000
                    and max(self.rss_hist[-3:]) - min(self.rss_hist[-3:]) < 2048):
                self.snapshot("a")
                self.snapshot_a_done = True
            # rolling end-of-run snapshot: /proc/<pid> vanishes the moment
            # the process exits, so "run end" = the last one taken alive
            if self.snapshot_a_done and ticks % 10 == 0:
                self.snapshot("b")
            if ticks % 3 == 0:
                self.sample_stat39()
            ticks += 1
            time.sleep(1.0)

    def finish(self):
        self.sample_stat39()
        self.snapshot("b")  # best effort; rolling copy above is the fallback
        self.stop_flag = True

def parse_numa_maps(text, measurement_only=False):
    """Page counts per node. measurement_only=True restricts to the pages
    membind/interleave actually governs and the workload actually streams:
    anonymous mappings (heap/stack/anon) and the tmpfs model file. File-backed
    executable/library mappings (bench binary + shared libs, ~10 MB) are
    node-fixed by page cache regardless of policy — the unfiltered gate
    structurally fails any cell bound away from the node caching them
    (observed: constant 162 foreign pages failing every N8b invocation)."""
    counts = {}
    for line in text.splitlines():
        if measurement_only:
            relevant = ("/dev/shm/opb" in line or "anon=" in line
                        or " heap" in line or " stack" in line)
            if not relevant:
                continue
        for tok in line.split():
            if tok.startswith("N") and "=" in tok:
                node, pages = tok[1:].split("=")
                counts[int(node)] = counts.get(int(node), 0) + int(pages)
    return counts

def busy_jiffies(cpu_lines, cpu):
    for line in cpu_lines:
        if line.startswith(f"cpu{cpu} "):
            f = list(map(int, line.split()[1:]))
            idle = f[3] + f[4]
            return sum(f) - idle, sum(f)
    return 0, 0

def sibling_check(cpus, seconds=10):
    """Busy fraction of SMT siblings of 1-thread-per-core cells."""
    sibs = sorted({base + k for base in cpus for k in (1, 2, 3)} - set(cpus))
    before = Path("/proc/stat").read_text().splitlines()
    time.sleep(seconds)
    after = Path("/proc/stat").read_text().splitlines()
    out = {}
    for s in sibs:
        b0, t0 = busy_jiffies(before, s)
        b1, t1 = busy_jiffies(after, s)
        frac = (b1 - b0) / max(t1 - t0, 1)
        out[s] = {"busy_frac": round(frac, 4),
                  "exempt_sacrificial": s in SACRIFICIAL}
    return out

# ---------- the run loop ----------

def invoke(cell, model, row, rnd, out_root):
    cpus, t, placement = CELLS[cell]
    r = reps_for(row, t)
    evdir = out_root / "evidence" / model / cell / f"round{rnd}"
    evdir.mkdir(parents=True, exist_ok=True)
    p_n = ("512", "0") if row == "pp512" else ("0", "128")
    argv = (numactl_prefix(placement, cpus)
            + [str(BENCH), "-m", str(staging_path(model, placement)),
               "-t", str(t), "-C", cpu_mask(cpus), "--cpu-strict", "1",
               "--delay", "3", "-r", str(r), "-o", "json",
               "-p", p_n[0], "-n", p_n[1]])
    if placement == "inter":
        # ARC regrows onto node 8 between rounds (evidence writes hit ZFS),
        # and --interleave silently falls back for anon allocations (KV,
        # compute buffers) — observed 60/40 measurement pages in AX round 2.
        balloon_node8(4)
    log(f"round {rnd} {cell} {model} {row} r={r}: {' '.join(argv)}")
    t_start = time.time()
    proc = subprocess.Popen(argv, stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)
    watcher = Watcher(proc.pid, cpus, evdir, row)
    watcher.start()
    sib = sibling_check(cpus, 10) if one_thread_per_core(cpus) else {}
    stdout, stderr = proc.communicate(timeout=3600)
    watcher.finish()
    time.sleep(0.1)
    wall = time.time() - t_start

    (evdir / f"bench.{row}.json").write_text(stdout)
    (evdir / f"bench.{row}.stderr").write_text(stderr[-8000:])
    (evdir / f"freq.{row}.csv").write_text(
        "\n".join(f"{ts},{c},{f}" for ts, c, f in watcher.freq))
    (evdir / f"stat39.{row}.csv").write_text(
        "\n".join(f"{ts},{tid},{c}" for ts, tid, c in watcher.stat39))
    if sib:
        (evdir / f"siblings.{row}.json").write_text(json.dumps(sib, indent=1))

    # gates
    gates = {"cmd": argv, "wall_s": round(wall, 1), "exit": proc.returncode}
    locality, full_loc, src_used = {}, {}, None
    for tag in ("b", "a"):
        snap = evdir / f"numa_maps.{row}.{tag}"
        if snap.exists() and snap.read_text().strip():
            text = snap.read_text()
            locality = parse_numa_maps(text, measurement_only=True)
            full_loc = parse_numa_maps(text)
            src_used = tag
            break
    gates["numa_snapshot_used"] = src_used
    total = sum(locality.values()) or 1
    excluded = {n: full_loc.get(n, 0) - locality.get(n, 0) for n in full_loc}
    if placement == "inter":
        frac0 = locality.get(0, 0) / total
        gates["numa"] = {"pages": locality, "node0_frac": round(frac0, 4),
                         "excluded_file_backed": excluded,
                         "ok": 0.45 <= frac0 <= 0.55}
    else:
        frac = locality.get(placement, 0) / total
        gates["numa"] = {"pages": locality, "bound_frac": round(frac, 4),
                         "excluded_file_backed": excluded,
                         "ok": frac >= 0.99}
    on_cpu = {c for _, _, c in watcher.stat39}
    gates["stat39"] = {"observed": sorted(on_cpu), "n_samples": len(watcher.stat39),
                       "ok": on_cpu.issubset(set(cpus)) and len(watcher.stat39) >= 5}
    want = ",".join(map(str, cpus)) if len(cpus) > 1 else str(cpus[0])
    gates["cpus_allowed"] = {"observed": sorted(watcher.allowed), "want": want}
    gates["ok"] = bool(proc.returncode == 0 and gates["numa"]["ok"] and gates["stat39"]["ok"])
    (evdir / f"attest.{row}.json").write_text(json.dumps(gates, indent=1))

    rows_out = []
    if proc.returncode == 0:
        for bench_row in json.loads(stdout):
            samples_ts = bench_row.get("samples_ts")
            if not samples_ts:
                n_tok = int(bench_row.get("n_prompt") or 0) + int(bench_row.get("n_gen") or 0)
                samples_ts = [n_tok / (ns / 1e9) for ns in bench_row["samples_ns"]]
            for ts_val in samples_ts:
                rows_out.append({"cell": cell, "block": rnd, "run": rnd,
                                 "metric": row, "value": ts_val})
    return gates["ok"], rows_out, wall

def env_manifest(out_root):
    m = {
        "date": time.strftime("%Y-%m-%dT%H:%M:%S%z"),
        "uname": run_out(["uname", "-a"]).stdout.strip(),
        "cmdline": read1("/proc/cmdline"),
        "numa_balancing": read1("/proc/sys/kernel/numa_balancing"),
        "thp": read1("/sys/kernel/mm/transparent_hugepage/enabled"),
        "irqbalance": run_out(["systemctl", "is-active", "irqbalance"]).stdout.strip(),
        "loadavg": read1("/proc/loadavg"),
        "bench": str(BENCH),
        "models": {k: v["sha256"] for k, v in MODELS.items()},
        "sacrificial_cpus": sorted(SACRIFICIAL),
    }
    (out_root / "env-manifest.json").write_text(json.dumps(m, indent=1))

def cmd_run(args):
    out_root = Path(args.out).expanduser()
    out_root.mkdir(parents=True, exist_ok=True)
    env_manifest(out_root)
    verify_staging()
    balloon_node8()  # headroom for AX interleaved runtime allocations
    models = args.models.split(",")
    cells = args.cells.split(",") if args.cells else list(CELLS)
    data_files = {m: open(out_root / f"pilot-{m}.jsonl", "a") for m in models}
    discards = []
    for rnd in range(args.rounds):
        combos = [(c, m) for c in cells for m in models]
        random.Random(20260901 + rnd).shuffle(combos)
        log(f"=== round {rnd}: {len(combos)} cell x model combos ===")
        for cell, model in combos:
            for row in ("pp512", "tg128"):
                for attempt in (1, 2):
                    ok, rows_out, wall = invoke(cell, model, row, rnd, out_root)
                    if ok:
                        for r_ in rows_out:
                            data_files[model].write(json.dumps(r_) + "\n")
                        data_files[model].flush()
                        break
                    discards.append({"round": rnd, "cell": cell, "model": model,
                                     "row": row, "attempt": attempt})
                    log(f"DISCARD round {rnd} {cell} {model} {row} attempt {attempt}")
        (out_root / "discards.json").write_text(json.dumps(discards, indent=1))
        log(f"=== round {rnd} complete ===")
    for fh in data_files.values():
        fh.close()
    log(f"pilot complete; discards: {len(discards)}")

def main():
    ap = argparse.ArgumentParser(description=__doc__)
    sub = ap.add_subparsers(dest="mode", required=True)
    sub.add_parser("preflight").set_defaults(fn=cmd_preflight)
    sub.add_parser("stage").set_defaults(fn=cmd_stage)
    rp = sub.add_parser("run")
    rp.add_argument("--rounds", type=int, default=5)
    rp.add_argument("--models", default="m1,m2")
    rp.add_argument("--cells", default="")
    rp.add_argument("--out", default="~/op-bench-pilot")
    rp.set_defaults(fn=cmd_run)
    args = ap.parse_args()
    args.fn(args)

if __name__ == "__main__":
    main()
