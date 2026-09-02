# Content plan: sections for openpower.tools

Research date: 2026-09-02 (web survey: RCS wiki, Talospace, OpenPOWER
Foundation, Mozilla/Fedora/box64/Hangover trackers, "are we X yet"
precedents). Sources at the end; statuses cited were sighted today and
must be re-verified when a section ships.

## Ecosystem findings that shape the plan

1. Browsers turned a corner in 2026: the SpiderMonkey ppc64le JIT is now
   a full port (Baseline + Ion, complete wasm incl. SIMD and JSPI,
   POWER8/9/10 paths, ~37.5k lines, ESR 153) by Trung Le (runlevel5),
   building on Cameron Kaiser's work; tested on Blackbird and P10; a
   Fedora copr exists and the Fedora maintainer plans to carry it in the
   official firefox package (153); Mozilla has invited a formal port
   proposal. Chromium is official in Fedora 40+/41+ and Debian, patches
   maintained by Raptor Engineering (gitlab.solidsilicon.io
   openpower-patches).
2. Emulation/gaming moved: box64 0.4.2 (2026-04) merged an initial
   ppc64le dynarec (usable, WIP; 64K page size hurts compatibility);
   Hangover gained PPC64 support (4K pages only; Wine PPC64 patches not
   upstreamed); UT2004 ships official ppc64el builds after Raptor
   offered hardware and porting help. QEMU user emulation remains the
   baseline.
3. Hardware: POWER9 (Talos II / Blackbird) is still the only
   owner-controlled platform. Power10 was skipped ecosystem-wide over
   firmware blobs. Power11 launched 2025-07-25 (IBM enterprise only).
   Solid Silicon S1 (ISA 3.1, DDR5/PCIe5, embedded X1 BMC, blob-free,
   "Talos III" working name on the RCS wiki) was quietly removed from
   Solid Silicon's site after 2025-02; credible rumours (Talospace) put
   a Raptor products-under-development announcement at Q1 2026 with
   "open firmware" explicitly planned; Raptor and Solid Silicon are now
   OpenPOWER Platinum members.
4. Foundation: LibreBMC (DC-SCM BMC on Microwatt/FPGA, Antmicro + Code
   Construct), Microwatt now ISA 3.1 SFFS-compliant and boots mainline
   Linux, Microwatt Momentum hackathon fabricated community designs via
   ChipFoundry. Repos migrating to the OpenPOWERFoundation GitHub org.
5. Risk watch: FreeBSD is considering retiring powerpc64 before 16;
   OpenBSD/powerpc64 is well-supported on Raptor hardware. Kernel page
   size (4K Debian vs 64K Fedora) is a recurring cross-cutting gotcha:
   KVM guest/host mismatches, box64/Hangover compatibility, historical
   browser issues.

## Proposed sections

### 1. "Can I use ... on POWER?" - the flagship (caniuse-style matrix)

Precedents: caniuse.com (UX), areweloongyet.com (a one-stop upstream
portal for LoongArch - the closest analogue), arewemodulesyet.org (data
model worth copying: per-item YAML records + generated data + merged
progress, explicit status legend, PR-driven updates).

- Data: one YAML record per item, in-repo, community-editable by PR:
  id, category, upstream links, per-axis statuses, evidence links
  (bugzilla/PR/copr/koji), notes, last-verified date.
- Axes that make this POWER-specific: arch (ppc64le, ppc64 BE),
  hardware generation (P8/P9/P10/P11), distro/channel (Fedora, Debian,
  Ubuntu, Gentoo, Void, Adelie, Chimera, FreeBSD/OpenBSD), and KERNEL
  PAGE SIZE (4K/64K) - the axis nobody else models and half the
  breakage hides behind.
- Statuses (textual, opt:badge variants, Okabe-Ito): upstream, patched
  downstream, in progress, broken, unsupported, unknown.
- Live probe panel: the one caniuse trick only we can do - the site
  already runs wasm on the visitor's machine, so a page can probe THEIR
  browser (wasm, SIMD, threads/COOP-COEP, JSPI, WebGPU, timer
  resolution) and render "your machine today" beside the matrix, using
  the same probes the benchmark protocol registered.
- Seed items from this survey: Firefox JIT/wasm/SIMD/JSPI; Chromium;
  WebKitGTK; Electron/VS Code; box64; Hangover/Wine; QEMU-user; KVM
  (page-size matrix); GCC/LLVM/Rust tier status; lld-required-for-
  linking notes; GPU drivers (amdgpu good, nouveau/nvidia caveats);
  desktop environments; toolchain oddities.

### 2. /platforms + /platform/{name}

Owner-controlled hardware pages (talos-ii, blackbird), reference IBM
systems (power10, power11) for context, and open cores
(microwatt, arctic-tern/kestrel, librebmc). Curated spec tables and
honest status ("what you can buy today vs what is rumoured"), linking
the RCS wiki rather than duplicating its depth. A tracked, carefully
sourced page for the S1/"Talos III" situation (announced 2023, site
delisting 2025, Q1 2026 rumour) would be widely read; it must separate
sourced fact from rumour explicitly.

### 3. /ports + /port/{name}

The porting-effort trackers engineers actually follow: firefox-jit
(timeline from TenFourFox lineage to ESR 153 + Fedora landing),
chromium, wine-hangover, box64-dynarec, ut2004 (the success story
template: hardware offer -> official builds), freebsd-risk. Each page:
current status, maintainers, how to help, hardware-access offers (a
recurring ecosystem pattern worth systematising: Raptor and community
members repeatedly offer dev boxes - a small "hardware for porters"
noticeboard has outsized leverage).

### 4. /status (extend the existing page)

Beyond the site's own build: aggregate ecosystem health - Fedora koji
and Debian buildd ppc64el excavation, copr trackers (firefox JIT),
FreeBSD port status, distro image freshness. This is the "aggregate
status information from distribution builds" purpose the status page
was created for.

### 5. /guides + /guide/{name}

Task-first: first-boot and firmware update paths per platform; the
page-size decision guide (4K vs 64K: KVM, box64/Hangover, browsers -
the single most asked cross-cutting question); building the JIT
Firefox; KVM on POWER9; GPU selection; petitboot recovery.

### 6. /firmware

The owner-control story told once, well: PNOR component map (skiboot/
hostboot/OCC/petitboot), BMC options (ASpeed/OpenBMC, Kestrel, Arctic
Tern, LibreBMC), flashing matrices, and what "blob-free" means per
platform generation (the P10 firmware story explains WHY the ecosystem
is shaped as it is).

### 7. /community

The curated map the RCS wiki keeps as raw links: forums, IRC/Matrix,
the blog ring (Talospace, Cat Fox Life, Store Halfword Byte-Reverse
Indexed, GNUcode, VivaPowerPC...), mailing lists. Optionally a
build-time RSS "radar" page aggregating recent posts (credit and link,
never republish).

### 8. /benchmarks (later)

Publish the registered-methodology results (op-ask LLM suite, STREAM,
AMO study) with the pre-registration story front and centre -
methodology-first benchmarking is a differentiator no other community
site attempts.

## Fit with the site's architecture

Matrix records are structured data -> rendered by opt components
(opt:badge for statuses, opt:table, opt:kpi for probe results) through
the validated XML pipeline; plural-index/singular-item URLs throughout
(/platforms -> /platform/talos-ii); statuses colour via --op-status-*
tokens (WCAG-checked in both themes). The live probe panel reuses the
wasm feature probes registered in the benchmark protocol.

## Sources (sighted 2026-09-02)

- RCS wiki Main Page (section inventory; compatibility lists; porting
  pages; Talos III and S1 pages) - wiki.raptorcs.com
- Talospace (CopyFail exploit note; FreeBSD powerpc64 retirement
  discussion; Debian 13 + page-size/KVM note; Power11 launch coverage
  with Q1 2026 Raptor rumour update) - talospace.com
- Mozilla Discourse "SpiderMonkey JIT with full WASM support for Power
  ISA" (2026-07); Bugzilla 1860412 (port history, Fedora landing plan,
  Mozilla port-proposal invitation); github runlevel5/firefox PR #1/#2
  (scope, test sweeps, P10 paths)
- osnews Power11 launch and "Firefox on POWER9: the JIT of it"
  (2026-01)
- Raptor/Solid Silicon/Lattice press (businesswire 2023-10); Phoronix
  2023-10; RCS wiki S1/Talos III pages (S1 delisting note)
- OpenPOWER Foundation: LibreBMC SIG page; librebmc repo docs;
  Microwatt repo (ISA 3.1 SFFS, mainline Linux); Microwatt Momentum
  hackathon results; linuxppc-dev Microwatt ISA 3.1 device-tree series
- box64 changelog 0.4.2 + issues #242/#946 (ppc64le dynarec, 64K page
  caveat); Hangover issue #20 (PPC64 support + 4K pages); OldUnreal
  UT2004 issue #316 (official ppc64el builds)
- Precedents: areweloongyet.com; arewemodulesyet.org (+ repo data
  model); awesome-areweyet index; repology as a data source for
  package-per-distro presence
