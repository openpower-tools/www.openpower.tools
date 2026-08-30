# OpenPOWER / Talos II landscape notes

Verified on 2026-08-31.

These are working notes gathered to inform the content of https://www.openpower.tools. They
are not authoritative. Every claim below carries the URL it came from and a short note of what
that page actually said on 2026-08-31.

Evidence markers used throughout:

- `[fetched]` — the page body was retrieved and read on 2026-08-31 (via Exa `web_fetch` or a
  direct HTTP request from this machine).
- `[snippet]` — only search-result highlights were seen, not the full page. Treat as weaker.
- `[dns]` / `[http]` — result of a direct DNS or HTTP probe run from this machine on 2026-08-31.

Where a source page is itself stale (a wiki that has not been edited in a while), that is noted
rather than silently repeated as current fact.

---

## 1. OpenPOWER Foundation

- Two hostnames serve the same Foundation site: https://openpower.foundation/ and
  https://openpowerfoundation.org/. Both render the same "Open Developer Community for the POWER
  Architecture" landing page claiming "more than 350 members".
  (https://openpower.foundation/ , https://openpowerfoundation.org/) `[fetched]`
- The site source is public: "Home of https://openpowerfoundation.org and
  https://openpower.foundation", a Hugo static site.
  (https://github.com/OpenPOWERFoundation/website-openpower.foundation) `[snippet]`
- ISA specification page lists released versions: 3.1c (2024-05-26, "Data cleanup"), 3.1b
  (2021-09-14), 3.1 (2020-05-02), 3.0c (2020-05-01), 3.0b (2017-03-29), 2.07b, 2.07. So the
  current published Power ISA is **3.1C**, dated May 2024; there has been no newer release as of
  today. (https://openpowerfoundation.org/specifications/isa/) `[fetched]`
- The same page lists "Approved RFCs for upcoming POWER ISA version": RFC02653 Dense Math
  Facility, RFC02654 SHA2 and SHA3 Compute Instructions, RFC02657 AES Acceleration Instructions.
  A next ISA revision is therefore in progress but unreleased.
  (https://openpowerfoundation.org/specifications/isa/) `[fetched]`
- Direct PDF of 3.1C: https://files.openpower.foundation/s/9izgC5Rogi5Ywmm/download/OPF_PowerISA_v3.1C.pdf
  `[snippet]` — referenced by both Wikipedia and a MorphZone thread; not downloaded here.
  A copy of the earlier 3.1B PDF is also mirrored on the Raptor wiki at
  https://wiki.raptorcs.com/w/images/d/d3/OPF_PowerISA_v3.1B.pdf `[snippet]`
- Licensing: the ISA is not under a normal open-source licence. It is distributed under the
  "OpenPOWER Power ISA End User License Agreement". Key granted rights per the EULA text:
  royalty-free perpetual copyright licence to develop Power-ISA-compatible products and to
  create/distribute "Power ISA Cores"; the right to license your own cores under CC-BY-4.0; and a
  royalty-free patent licence to make and sell "Power Compliant Chips". Section 1.4 explicitly
  withholds the right to implement a *non-compliant* core in physical silicon or an FPGA, and
  section 1.6 is a patent-litigation termination clause.
  (https://openpowerfoundation.org/blog/final-draft-of-the-power-isa-eula-released/) `[snippet]`
  — the same clause text also appears verbatim in the front matter of the 3.1B PDF. This is the
  practical shape of "open" here: open to implement, gated on compliance, not a free-software
  licence.
- Governance: OpenPOWER Foundation became a project hosted at the Linux Foundation, announced
  2019-08-20/21. It keeps its own board and decision making but operates under LF open governance.
  (https://www.prnewswire.com/news-releases/the-linux-foundation-announces-new-open-hardware-technologies-and-collaboration-300904869.html)
  `[snippet]`; corroborating contemporaneous write-up at https://lwn.net/Articles/796796/ `[snippet]`
- A Technical Steering Committee exists, composed of Work Group chairs plus a delegate from each
  Platinum member, two-year terms. (https://openpowerfoundation.org/steeringcommittee/) `[snippet]`
- Recent Foundation activity is thin. The blog index's two most recent posts are
  "OPF Announces Winners of Microwatt CPU Hackathon" (2025-12-09) and "OPF Announces New AI
  Special Integration Group" (2025-07-11). Nothing newer is listed as of today.
  (https://openpowerfoundation.org/blog/) `[fetched]`
- Microwatt Design Challenge / "Microwatt Momentum" hackathon, run with ChipFoundry: 300+ teams
  registered, three winning designs selected, to be fabricated by ChipFoundry and delivered as
  packaged parts plus eval boards; other qualifying entrants get a free eval board. Announced
  2025-12-18. (https://www.linuxfoundation.org/press/openpower-foundation-and-chipfoundry-announce-winners-of-the-microwatt-design-challenge-advancing-open-source-power-hardware
  and https://openpowerfoundation.org/blog/opf-announces-winners-microwatt-hackathon/ and
  https://chipfoundry.io/challenges/microwatt) `[snippet]`
- Note for our own link hygiene: `git.openpower.foundation` **now redirects to GitHub**. A live
  request to https://git.openpower.foundation/ ends at https://github.com/OpenPOWERFoundation ,
  and https://git.openpower.foundation/cores/microwatt/ ends at
  https://github.com/OpenPOWERFoundation/microwatt . `[http]` The OPF-hosted Git service appears to
  have been retired in favour of GitHub; many older docs still cite the `git.openpower.foundation`
  URLs, which still work as redirects.

## 2. Raptor Computing Systems

- Product line on the site today is still POWER9 only: Talos II (workstation, 4U server, two 2U
  storage servers, desktop dev system, entry-level dev system, PowerAI dev system, single- and
  dual-CPU bundles), Talos II Lite mainboard, Blackbird (Secure Desktop, mainboard, 4-core and
  8-core bundles), plus POWER9 Sforza CPUs in 4/8/18/22-core and heatsink parts.
  (https://www.raptorcs.com/ , https://www.raptorcs.com/TALOSII/ ,
  https://www.raptorcs.com/TALOSIILITE/ , https://www.raptorcs.com/BB/) `[fetched]`
- **Availability is the headline.** Live fetch of the Talos II and Blackbird product listings
  today shows *every* SKU marked `Current Status: Out of Stock (Special Order)` — all 18 Talos II
  line items and all 8 Blackbird line items. Individual product detail pages agree, e.g.
  https://www.raptorcs.com/content/TL2SV3/intro.html says "Out of Stock (Special Order) — This
  item cannot be purchased at this time". `[http]` Note that Exa's cached copies of some of these
  pages said "Full Production"; the live pages do not. Prefer the live state.
- Arctic Tern soft-BMC module (AT1MB2) is also `Out of Stock (Special Order)`, "This item cannot
  be purchased at this time". (https://www.raptorcs.com/content/AT1MB2/intro.html) `[http]`
- Indicative pricing seen live today: Talos II Secure Workstation from $11,966.99; Talos II
  mainboard $4,980.88; Talos II Lite mainboard $2,964.14; Blackbird mainboard $2,342.99;
  Blackbird Secure Desktop $6,544.99; 8-core POWER9 $1,491.53; 22-core POWER9 $5,457.38.
  (https://www.raptorcs.com/TALOSII/ , https://www.raptorcs.com/BB/) `[http]`
- Support: "Most support documentation is available on each product's Web page", contact
  support@raptorcs.com; sales@raptorcs.com for orders/POs; 2-year limited warranty. Notably,
  "firmware upgrades of any kind are not covered under warranty, but may be available as a
  separate, paid convenience service". (https://www.raptorcs.com/content/base/support.html)
  `[fetched]`
- https://www.raptorcs.com/content/base/software.html is effectively an empty stub ("Software /
  Related Products" and nothing else). `[http]` Do not link it as a software index.
- Power10 openness problem: Raptor did not build Power10 machines because Power10 required
  closed-source firmware in parts of the design, specifically the OMI memory interface, and
  reportedly Synopsys I/O IP. Talospace's own framing: Solid Silicon's S1 is "a fully blob-free
  OpenPOWER successor to the POWER9, avoiding Power10's notorious binary firmware requirement for
  OMI RAM and I/O". (https://www.talospace.com/2023/10/the-next-raptor-openpower-systems-are.html)
  `[snippet]`. On Power11, Talospace states plainly: "We don't know if the OMI firmware for
  Power11 is open and libre (it was not in Power10), nor if the Synopsys IP blocks reportedly used
  in Power10's I/O are still present, because IBM didn't say."
  (https://www.talospace.com/2025/07/power11-hits-market-this-month.html) `[fetched]`
- Announced next platform (2023): Raptor + Solid Silicon + Lattice press release. Solid Silicon
  to be Raptor's future CPU provider; S1 CPU with embedded Power ISA 3.1-compliant server-class
  cores as a "fully owner-controlled, blob-free successor" to POWER9, with an embedded X-class
  BMC; "expected availability of systems by late 2024"; Lattice-based X1 chip expected H2 2024;
  Arctic Tern dev kit available now, "Antarctic Tern" X1-powered kit expected late 2024.
  (https://www.businesswire.com/news/home/20231019873615/en/...) `[snippet]`
- S1 specifications as reported by Talospace after asking Timothy Pearson: PCIe 5.0, DDR5 (no OMI
  required), high-3GHz to low-4GHz, bi-endian, SMT-4, at least two sockets, 18-core option
  confirmed. X1 BMC is a Microwatt-based ISA 3.1 core on Lattice ECP5/iCE40 with 512MB DDR3.
  (https://www.talospace.com/2023/10/the-next-raptor-openpower-systems-are.html) `[snippet]`
- **None of that shipped.** The Raptor wiki S1 page says: "The S1 was removed from Solid Silicon's
  website sometime after 2025 Feb 9. The reasons for removal are unknown."
  (https://wiki.raptorcs.com/wiki/S1) `[fetched]`. "Talos III" is only a working name for the
  Power ISA v3.1 platform using S1 + X1. (https://wiki.raptorcs.com/wiki/Talos_III) `[fetched]`
- **Solid Silicon's web presence is gone.** As of today `solidsilicon.com`, `www.solidsilicon.com`,
  `solidsilicon.io` and `gitlab.solidsilicon.io` return no DNS records at all (no A, no NS) from
  both the local resolver and 8.8.8.8. `[dns]` This matters practically: the Chromium ppc64le
  patch set used to live at `gitlab.solidsilicon.io/public-development/open-source/chromium/openpower-patches`
  and distributions have had to move off it (see §5).
- Rumoured Raptor announcement: Talospace appended an update to its Power11 post saying an
  anonymous source with knowledge indicated "a new Raptor announcement on products under
  development is scheduled for Q1 2026", with "open firmware ... specifically mentioned and
  absolutely planned", and noting both Raptor and Solid Silicon are listed as Platinum OpenPOWER
  members. It is explicitly flagged as not official.
  (https://www.talospace.com/2025/07/power11-hits-market-this-month.html) `[fetched]`
  **Not verified: no such announcement was found on raptorcs.com or the forums as of today.**
- Wiki: https://wiki.raptorcs.com/wiki/Main_Page — community-driven MediaWiki covering Raptor
  hardware and OpenPOWER/PowerNV/POWER9 generally. Has quick-start guides, per-platform pages,
  memory and PCIe compatibility lists, an OS compatibility list, a game compatibility list, a
  "Fixes in Progress" upstreaming tracker, firmware pages, and porting pages for Firefox, Chromium
  and Tor Browser. It also carries a curated list of community blogs. This is the single most
  useful existing community resource. `[fetched]`
- Firmware releases: the current official Raptor "System Package" is **v2.10, released
  2024-02-15**, for both Talos II and Blackbird. Nothing newer is listed.
  (https://wiki.raptorcs.com/wiki/Talos_II/Firmware — fetched live, lists v2.10 then v2.00
  (2020-02-19) and the 1.0x series; https://wiki.raptorcs.com/wiki/Blackbird/Firmware — v2.10,
  v2.00a (2021-10-29), v2.00, v1.00) `[fetched]`
- v2.10 release notes (PNOR): hostboot/skiboot/petitboot/hcode updated to latest upstream; skiroot
  moved to Linux 6.6.y; Infiniband drivers enabled in skiroot; fix for a sporadic crash on
  paired-core (>8 core) machines; hostboot runtime compressed to free BOOTKERNFW space; firmware
  component signature checks enabled by default during IPL using a well-known *insecure*
  transitional public key, to ease transition to an owner key.
  (https://wiki.raptorcs.com/wiki/Talos_II/Firmware/2.10/Release_Notes) `[snippet]`
  Forum thread confirms BOOTKERNFW grew to roughly 5MB in v2.10, which is what makes it possible
  to stuff modern AMD GPU firmware in there.
  (https://forums.raptorcs.com/index.php?topic=531.0) `[snippet]`
- Upgrade procedure: https://wiki.raptorcs.com/wiki/Firmware_Upgrade_Quickstart — three firmware
  components (system control FPGA bitstream, BMC stack, host PNOR); from System Package v2.00 the
  BMC web UI has a point-and-click firmware page; SSH path is `scp` the `.pnor` to the BMC then
  `pflash -E -p /tmp/<file>` plus `pflash -P CVPD -c`. Raptor's own text encourages building from
  source over using their prebuilt images. `[snippet]`
- **Source hosting has moved and the old host is stale.** `git.raptorcs.com` (cgit) still serves,
  but its repositories are idle 2 to 7 years: `talos-xml` 2 years, everything else 5-7 years,
  including `talos-hostboot`, `talos-op-build`, `talos-openbmc`, `blackbird-*`.
  (https://git.raptorcs.com/git/?s=idle) `[fetched]`
  The Raptor wiki's current build instructions instead point at
  `https://gitlab.raptorengineering.com/openpower-firmware/machine-talos-ii/op-build.git` (branch
  `raptor-v2.10`) and `.../machine-blackbird/op-build.git`, and warn that `master` "is often in a
  non-functional state". (https://wiki.raptorcs.com/wiki/Compiling_Firmware) `[fetched]`
- **Warning: gitlab.raptorengineering.com was returning HTTP 500/502 from this machine today.**
  Live probes of https://gitlab.raptorengineering.com/ and of the `machine-talos-ii/op-build`
  project both returned an Apache "502 Proxy Error / Error reading from remote server". `[http]`
  An Exa-cached render of the `openpower-firmware` group page did come back, showing
  "Group ID: 17" with no publicly listed projects. `[fetched, cached]` So the canonical firmware
  source is currently either intermittently down or not publicly browsable. Anything we publish
  should note this and offer a fallback.
- Also on that GitLab footer: "Powered by Integricloud. Any automated access to this website for
  the purpose of training any LLM ("AI") for non-personal use ... may be billed to the accessor per
  the Terms of Service." Worth respecting; do not build any scraper/mirror against it.
- Community forums: https://forums.raptorcs.com/ — still marked "(BETA)". Live board index shows
  recent activity: last post in "Operating Systems and Porting" 2026-08-17, "Software" 2026-08-16,
  hardware boards early August 2026. Boards include Water Cooler, Operating Systems and Porting,
  Software, Raptor hardware, General OpenPOWER Hardware, OpenPOWER ISA, Third Party CPU
  Discussion (with a Microwatt child board). `[fetched]`
- IRC/Matrix: `#talos-workstation` on **Libera.Chat**, bridged to OFTC `#talos-workstation` and
  hackint `#talos-workstation`, and to Matrix `#talos-secure-workstation:matrix.org`. The wiki
  notes Libera's own Matrix bridge is currently disabled and to use the native Matrix room instead,
  and that you must register and identify your nick or you get "you are banned".
  (https://wiki.raptorcs.com/wiki/IRC) `[fetched]`

## 3. POWER9 firmware stack (Talos II / Blackbird)

The Raptor wiki page https://wiki.raptorcs.com/wiki/OpenPOWER_Firmware `[fetched]` is the clearest
single description of the boot chain and is the basis for most of this section. Boot order it
gives:

1. OpenBMC uses the FSI interface to start the SBE.
2. SBE executes OTPROM (burned into POWER9 silicon via eFuses), which loads SEEPROM firmware into
   SBE PIBMEM.
3. SBE executes SEEPROM firmware, initialises a CPU core, loads the Hostboot Bootloader (HBBL).
4. HBBL loads and verifies Hostboot; Hostboot does DRAM training, ECC zeroing, processor bus and
   memory buffer init.
5. Hostboot chainloads Skiboot, which initialises PCIe, device trees, RTC, NVlink, sensors, loads
   and starts OCC, and implements OPAL runtime services (remaining resident after OS boot).
6. Skiboot chainloads Skiroot (kernel + initramfs containing Petitboot).
7. Petitboot `kexec`s the operating system; the OS then talks to firmware through OPAL.

Component notes and canonical repos:

- **Skiboot / OPAL** — https://github.com/open-power/skiboot , Apache-2.0, default branch `master`.
  README describes OPAL split into skiboot plus skiroot (kernel + petitboot initramfs). Mailing
  list skiboot@lists.ozlabs.org, patchwork at http://patchwork.ozlabs.org/project/skiboot/list/ ,
  docs at http://open-power.github.io/skiboot/doc/index.html . 111 stars, 67 open issues.
  `[fetched]`
- **Hostboot** — https://github.com/open-power/hostboot , Apache-2.0. Default branch is
  `master-p10`, and the README's first line is "** NOTE : This branch is deprecated. Use one of the
  named release branches for up to date code **". Development has clearly moved to Power10 release
  branches; POWER9 users should be on Raptor's branch, not this default.
  `[fetched]`
- **op-build** — https://github.com/open-power/op-build , GPL-2.0, Buildroot overlay that builds
  Hostboot, Skiboot, OCC, Petitboot into a PNOR image. README still documents
  `./op-build blackbird_defconfig && ./op-build`, lists POWER9 platform defconfigs (Witherspoon,
  Boston/p9dsu, Romulus, Zaius), and requires Python 2.7 plus a Fedora-33 or Debian-12/Ubuntu-22.04
  era host — it now recommends running the build inside a Fedora 33 Toolbx container. That toolchain
  age is itself a signal of maintenance state. Mailing list openpower-firmware@lists.ozlabs.org
  (https://lists.ozlabs.org/listinfo/openpower-firmware). `[fetched]`
- **SBE / OCC / HCODE / CME / SGPE / PGPE / IOPPE** — described on the wiki page above. SBE has an
  OTPROM part (first instructions, unchangeable) and a rewritable SEEPROM part, with a backup copy
  on PNOR used to stage SBE updates. OCC handles thermal regulation, turbo/VID selection and power
  measurement, and runs on a dedicated on-die core. CMEs are auxiliary cores, one per pair of SMT4
  cores, subordinate to the OCC. SGPEs handle resume-from-STOP, PGPEs handle pstate management.
  IOPPE is involved in CAPI. `[fetched]`
  Raptor's own forks of `talos-sbe`, `talos-hcode`, `talos-occ` are on git.raptorcs.com but idle
  6-7 years (https://git.raptorcs.com/git/?s=idle) `[fetched]`; the live code path is the
  gitlab.raptorengineering.com op-build superproject.
- **Petitboot / skiroot** — Petitboot is the userspace boot menu inside skiroot; it `kexec`s the
  target OS. Raptor's `talos-petitboot` mirror is 6 years idle. `[fetched]`
- **OpenBMC** — https://github.com/openbmc/openbmc , Yocto/OpenEmbedded distro for BMCs. Last push
  recorded 2026-06-10, latest tagged release 2.18.0 (2025-05-30), 350 contributors. Actively
  maintained upstream. `[fetched]` Talos II and Blackbird ship OpenBMC on an ASPEED AST2500.
  (https://wiki.raptorcs.com/wiki/Talos_II) `[fetched]`
  Related component: https://github.com/openbmc/openpower-occ-control — "Control application for
  the OpenPOWER On-Chip-Controller", handles OCC comms for temperatures, power readings, power
  caps, power modes. `[snippet]`
  Raptor's OpenBMC fork build instructions on the wiki still say
  `git clone -b raptor-v1.07 https://git.raptorcs.com/git/talos-openbmc` — that is out of date
  relative to System Package v2.10. (https://wiki.raptorcs.com/wiki/Compiling_Firmware) `[fetched]`
- **Kestrel soft BMC** — Raptor Engineering's open-HDL/open-firmware BMC, announced January 2021,
  built on the Microwatt soft core (little-endian) targeting Lattice ECP5, developed entirely with
  open FPGA tooling (Yosys/nextpnr/OpenOCD) on OpenPOWER hosts. It implements FSI, SPI, LPC, I2C,
  VUART and can IPL a POWER9 host with the ASPEED BMC out of the loop.
  Announcement to the OpenBMC list: https://lists.ozlabs.org/pipermail/openbmc/2021-January/024726.html
  `[snippet]`. Repos under https://gitlab.raptorengineering.com/kestrel-collaboration/ (e.g.
  `kestrel-litex/litex-boards`, README last touched 2025-04-28 per search metadata) `[snippet]`.
  Coverage: https://www.talospace.com/2021/01/introducing-kestrel-part-ii-its-soft-bmc.html and
  https://www.phoronix.com/news/Raptor-Kestrel `[snippet]`.
- **Arctic Tern** — the physical FPGA card that runs Kestrel. Lattice ECP5, ModBMC-compatible SFF
  card in a DDR4 SODIMM socket, integrated GbE and HDMI, runs either bare-metal firmware or Zephyr
  RTOS, and supports either Microwatt or Libre-SOC as the soft CPU.
  (https://www.raptorcs.com/content/AT1MB2/intro.html) `[fetched]`
  Integration guide PDF (v0.90) on the wiki:
  https://wiki.raptorcs.com/w/images/b/b9/Arctic_Tern_BMC_Integration_Guide_Version_0.90.pdf —
  documents installing the card into a Talos II, cabling LPC/FSI/PMBus/AVSBus, building the HDL
  with `./rcs_arctic_tern_bmc_card.py --device=LFE5UM --cpu-type=microwatt ...`, and states plainly
  that "some features are not yet enabled, for example full Redfish support and VGA graphics
  output". The guide assumes a Debian 11 ppc64el build host. `[snippet]`
  Wiki page: https://wiki.raptorcs.com/wiki/Arctic_Tern — notes Raptor does not publish a
  downloadable bitstream, you compile from source, and Debian 11 is recommended. `[snippet]`
  Current purchase status: out of stock, cannot be purchased. `[http]`
- **LibreBMC** — a separate OpenPOWER Foundation effort (announced 2021-05-10, showcased at OCP
  2022) for a POWER-based fully open-source BMC, distinct from Kestrel; Kestrel is Raptor in-house
  and Zephyr/LiteX-based, LibreBMC is OpenBMC-based.
  (https://openpowerfoundation.org/blog/ index entries; discussion at
  https://www.talospace.com/2021/05/librebmc-and-kestrel-two-separate-bmc.html) `[snippet]`
- **coreboot + Heads as an alternative to Hostboot + Petitboot on Talos II** — this is the most
  interesting recent firmware work and it is real and documented. Krystian Hebel (3mdeb),
  published 2024-10-08 on the OPF blog. coreboot replaces Hostboot and hands off to Skiboot as its
  payload; Heads replaces Skiroot/Petitboot. Three years of work, funded via Open Collective,
  Insurgo, 3mdeb and the NGI0 PET fund, shipped as part of the Dasharo distribution. Build is
  `git clone https://github.com/Dasharo/coreboot.git -b raptor-cs_talos-2/rel_v0.7.0` and
  `git clone https://github.com/Dasharo/heads.git -b raptor-cs_talos-2/release`, flashed with
  `pflash -e -P HBB` / `-P HBI` from the BMC. Requires System Package **v2.00** specifically
  (the post says update or *downgrade* to it), plus a TPM and a USB security dongle for Heads.
  (https://openpowerfoundation.org/blog/coreboot-on-talos2/) `[fetched]`
- Reproducible builds: the coreboot/Heads work notes `BUILD_TIMELESS` is always enabled for Heads
  "for security and reproducible images", but that this strips file names and line numbers from
  asserts, so 3mdeb suggests using a manually built coreboot for debugging.
  (https://openpowerfoundation.org/blog/coreboot-on-talos2/) `[fetched]`
  **Not verified: I found no organised, current "rebuild Raptor firmware bit-for-bit reproducibly"
  project.**
- Testing tooling: https://github.com/open-power/op-test — out-of-band automated test suite for
  OpenPOWER machines (power cycling, boot configurations, fwts/HTX on host). Docs at
  http://open-power.github.io/op-test/ `[snippet]`
- Blob status on Talos II/Blackbird: the mainboard uses a Broadcom BCM5719 NIC (Raptor's
  "Project Ortega" reimplemented its firmware — https://git.raptorcs.com/git/bcm5719-ortega/ , idle
  5 years `[fetched]`), a firmware-free TI TUSB7340 XHCI, and an *optional* Microsemi PM8068 SAS
  controller. Talospace stated in 2021 that after the Broadcom work "the Microsemi PM8068 [is] the
  last blob firmware component and only if you buy it as a BTO option".
  (https://wiki.raptorcs.com/wiki/Talos_II `[fetched]` ;
  https://www.talospace.com/2021/05/librebmc-and-kestrel-two-separate-bmc.html `[snippet]`)
- Overall maintenance read: the upstream `open-power` GitHub org still exists and skiboot is alive,
  but hostboot's default branch is self-declared deprecated and op-build targets a Python-2-era
  host. Raptor's platform trees are the practical source of truth and they last cut a release in
  February 2024. This is a stack in low-maintenance mode, not an abandoned one.

## 4. Community

- **Talospace** — https://www.talospace.com/ , by Cameron Kaiser (ClassicHasClass). The de facto
  news blog for OpenPOWER desktops. Live RSS check today shows the most recent post is
  "CopyFail works on ppc64le" dated 2026-04-30, preceded by "FreeBSD considering end of ppc64
  support" (2025-11-21), "Debian 13 Trixie" (2025-08-12), "Power11 hits the market this month"
  (2025-07-08), "Enter the IBM z17 mainframe with Telum II" (2025-04-08), "Plan 9 finally comes to
  the POWER9" (2025-04-01). So: still the best source, but posting cadence has dropped to a
  handful of posts a year, and there has been nothing new for four months.
  (https://www.talospace.com/feeds/posts/default?alt=rss) `[http]`
- **Raptor community forums** — https://forums.raptorcs.com/ , active (last posts mid-August 2026).
  Boards for OS/porting, software, hardware, OpenPOWER ISA, and third-party CPUs. `[fetched]`
- **IRC / Matrix** — Libera.Chat `#talos-workstation`, bridged to OFTC and hackint, plus Matrix
  `#talos-secure-workstation:matrix.org`. Libera's own Matrix bridge is disabled; use the native
  room. (https://wiki.raptorcs.com/wiki/IRC) `[fetched]`
- **Mailing lists** (all on lists.ozlabs.org):
  - linuxppc-dev — Linux on PowerPC development, linked from the RCS wiki main page. `[fetched]`
  - skiboot — https://lists.ozlabs.org/listinfo/skiboot `[fetched via skiboot README]`
  - openpower-firmware — https://lists.ozlabs.org/listinfo/openpower-firmware . Its description:
    "primarily for op-build ... and for discussion on things that otherwise don't have a home."
    `[snippet]`
  - openbmc — https://lists.ozlabs.org/pipermail/openbmc/ `[snippet]`
- **Other community blogs**, all listed on the RCS wiki main page External Links section `[fetched]`:
  The Cat Fox Life; "Store Halfword Byte-Reverse Indexed" (a Power technical blog); Collection of
  Bits (OpenPower/OpenBMC/Linux notes); Stewart Smith's Ramblings; VivaPowerPC; This Is
  Apfelhammer!; PowerPC Liberation; PPC Luddite; GNUcode.org. Also reddit r/OpenPOWER and
  r/PowerPC, and the Level1Techs POWER/PowerPC thread.
- **Microwatt** — tiny Power ISA softcore in VHDL-2008. Original repo
  https://github.com/antonblanchard/microwatt (721 stars, top contributors paulusmack,
  antonblanchard, ozbenh, mikey). `[fetched]` The Foundation now also publishes
  https://github.com/OpenPOWERFoundation/microwatt (created 2026-05-13) `[snippet]`, and
  https://git.openpower.foundation/cores/microwatt/ redirects there `[http]`. A GitLab read-only
  mirror exists at https://gitlab.com/openpowerfoundation/microwatt `[snippet]`. Per a MorphZone
  post quoting linuxppc-dev, Microwatt now supports the SFFS compliancy subset of Power ISA 3.1C
  and is SMP-capable `[snippet]` — **not independently verified here.**
- **A2I / A2O open cores** — https://github.com/openpower-cores/a2i is **ARCHIVED**; its README
  says "This repo has been archived and relocated. The new home is:
  https://git.openpower.foundation/cores/a2i . It is mirrored at:
  https://github.com/OpenPOWERFoundation/a2i". A2I is a 4-threaded in-order core, Power ISA 2.06
  Book III-E, the BlueGene/Q core. `[fetched]` A companion A2O (out-of-order) repo exists under the
  same orgs. A MorphZone post quotes an OPF call for interns to bring A2O to Power ISA 3.0C/3.1C
  compliance (radix MMU, VMX/VSX, Book III-S interrupt model, `scv`) `[snippet]` — **the actual
  status of that internship programme is not verified.**
- **Libre-SOC** — https://libre-soc.org/ , hybrid CPU/VPU/GPU on Power ISA with the SVP64 vector
  extension, NLnet/NGI-funded, self-hosted infrastructure (git.libre-soc.org), explicitly not on
  GitHub. `[snippet]` However Wikipedia records that "In a list-serv message dated June 23, 2024,
  project lead, Luke Kenneth Casson Leighton described the project as 'effectively terminated'."
  (https://en.wikipedia.org/wiki/Libre-SOC) `[snippet]` — **the primary list-serv message was not
  located or read; treat the status as uncertain.** The Open Collective page
  (https://opencollective.com/libre-soc) shows a credit dated 2026-08-01, so something is still
  moving financially. `[snippet]`
- **Solid Silicon** — see §2. No working web presence today. `[dns]`
- **powerpc-notebook / Power Progress Community** — https://www.powerpc-notebook.org/en/ . This is
  an NXP-based (not POWER9) effort but is the other live open-hardware PowerPC community. Latest
  news post 2026-08-15: prototype campaign reduced from five to three prototypes, goal lowered from
  EUR 11,000 to EUR 9,100; contract signed with a manufacturer; "Powerboard Tyche" desktop board,
  reusing much of the 2023 NXP T2080RDB reference design plus their own USB3/audio/dual-RAM-slot
  work; NXP reviewed the design in January 2026; prototype production setup starting 2026-08-24
  with prototypes targeted for end of September 2026. Run jointly by Power Progress Community ODV
  and ACube Systems. `[fetched]`
- **3mdeb / Dasharo** — see §3; the coreboot+Heads Talos II port. Repos:
  https://github.com/Dasharo/coreboot (branch `raptor-cs_talos-2/rel_v0.7.0`) and
  https://github.com/Dasharo/heads (branch `raptor-cs_talos-2/release`). `[fetched via OPF blog]`
- **ChipFoundry** — https://chipfoundry.io/challenges/microwatt , ran the Microwatt Momentum
  hackathon with OPF; will fabricate the three winning designs on SKY130 via the Caravel/OpenFrame
  platform. Winning repo referenced: https://github.com/Lefteris-B/microwatt_design_challenge
  `[snippet]`
- **Oregon State University Open Source Lab** — hosts build infrastructure for several of these
  projects (Chimera's ppc64/ppc builders run on an OSUOSL VM; OSUOSL sponsored POWER8/POWER10
  hardware for the Firefox JIT work). OSUOSL's director sits on the OPF Technical Steering
  Committee. (https://chimera-linux.org/news/2026/03/retiring-powerpc.html `[fetched]`;
  https://github.com/runlevel5/firefox-ppc64/pull/2 `[snippet]`;
  https://openpowerfoundation.org/steeringcommittee/ `[snippet]`)
- **Not verified:** I did not find any *new* 2025-2026 community wiki or portal dedicated to
  ppc64le that would compete with the RCS wiki. That gap is arguably the opening for this project.

## 5. OS and toolchain port status

### Linux distributions (little-endian ppc64le unless noted)

- **Debian** — `ppc64el` is an *official released* architecture, added in Debian 8, described as
  "Port for the 64-bit little-endian POWER architecture, using the new Open Power ELFv2 ABI",
  baseline POWER7+/POWER8. (https://www.debian.org/ports/) `[fetched]`
  Big-endian `ppc64` and 32-bit `powerpc` are **not** official; they live on Debian Ports
  (https://www.ports.debian.org/ , live today, lists `ppc64` and `powerpc` among hosted
  architectures). `[http]` Talospace's Debian 13 Trixie post (2025-08-12) covers the current
  release. (https://www.talospace.com/2025/08/debian-13-trixie.html) `[snippet]`
- **Fedora** — ppc64le is a supported alternate architecture with builds merged into primary Koji
  since 2016. The Fedora wiki page states "Only 64bit machines (little endian Power8 or newer) are
  supported now" and names Dan Horák, Mark Hamzy, Mike Wolf and Aditi Mishra as PowerPC SIG
  members; IRC `#fedora-ppc` on Libera. (https://fedoraproject.org/wiki/Architectures/PowerPC)
  `[fetched]` **Caveat: that wiki page still says Fedora 43 (2025-10-28) is the latest stable,
  which is stale** — the RCS OS compatibility list records Fedora 44 as working on ppc64le, and the
  Firefox JIT work references building on Fedora 44. Treat the Fedora wiki as behind.
- **Ubuntu** — `ppc64el` is one of Canonical's officially supported architectures ("Build failures
  on these architectures are considered serious bugs"). The POWER download page says POWER9 and
  POWER10 supported from 22.04 LTS, POWER11 supported from 24.04 LTS, "Ubuntu 26.04 LTS is
  recommended", and POWER8's last release was 20.04 LTS. Baseline moved from POWER8 to POWER9 at
  22.04 LTS. (https://ubuntu.com/download/server/power `[fetched]`;
  https://ubuntu.com/project/docs/how-ubuntu-is-made/concepts/supported-architectures/ `[snippet]`)
  Ubuntu 26.04 LTS "Resolute Raccoon" released 2026-04-23 on Linux 7.0; ppc64el packages are
  publishing normally (kernel 7.0.0-30.30 published to resolute/ppc64el security on 2026-08-20).
  (https://canonical.com/blog/canonical-releases-ubuntu-26-04-lts-resolute-raccoon ,
  https://answers.launchpad.net/ubuntu/resolute/ppc64el/linux-headers-virtual-hwe-26.04-edge)
  `[snippet]`
- **Void Linux for PowerPC (void-ppc)** — **discontinued.** The maintainer (Daniel Kolesa / q66)
  announced maintenance would cease from January 2023 in favour of Chimera Linux, with the public
  repository hosting shutting down.
  (https://voidlinux-ppc.org/news/project-status-update-for-2023/) `[snippet]`
  The website https://voidlinux-ppc.org/ still resolves and returns HTTP 200 today, but
  `repo.voidlinux-ppc.org` no longer resolves in DNS. `[http]` `[dns]`
  The RCS OS compatibility list carries the note "Void Linux for Power ISA has been discontinued in
  January 2023 in favor of Chimera Linux."
  (https://wiki.raptorcs.com/wiki/Operating_System_Compatibility_List) `[fetched]`
  GitHub org https://github.com/void-ppc shows last activity 2023. `[snippet]`
- **Chimera Linux** — ppc64le is a **tier 1** architecture alongside x86_64 and aarch64 and is
  *not* affected by any retirement. Big-endian is a different story: on 2026-03-13 Chimera
  announced a plan to retire 32-bit `ppc` and big-endian `ppc64`, targeting July 2026, citing no
  dedicated maintainer, years-long Mesa regressions forcing use of mesa-amber, no up-to-date web
  browser, and a single shared OSUOSL VM as the builder. The offer to retract stands if someone
  takes it over. (https://chimera-linux.org/news/2026/03/retiring-powerpc.html) `[fetched]`
  **Open question:** the Chimera news index as of today shows that March post as the most recent
  item, with no follow-up confirming whether the July 2026 removal actually happened.
  (https://chimera-linux.org/news/) `[fetched]`
- **Gentoo** — active PowerPC project covering both 32-bit and 64-bit, both endians. Named
  arch-team members include ago, arthurzam, blueness, lu_zero, sam. IRC `#gentoo-powerpc` on
  Libera; mailing lists gentoo-ppc-user and gentoo-ppc-dev; two dev boxes (`timberdoodle` ppc64be,
  `bogsucker` ppc64le). Documents both 4K and 64K page-size kernels, and ppc64be ABI options
  (glibc/ELFv1 default, musl/ELFv2 only). (https://wiki.gentoo.org/wiki/Project:PowerPC) `[fetched]`
- **Alpine** — ppc64le is a live build architecture. The package index shows `firefox 154.0-r0`
  built for ppc64le in `edge/community` on **2026-08-29**, i.e. two days ago.
  (https://pkgs.alpinelinux.org/packages?name=firefox&arch=ppc64le) `[fetched]`
- **Adélie Linux** — https://www.adelielinux.org/ , independent musl-based distro; the front page is
  live and pitches "your hardware, whether it's from 1995 or 2025". The RCS OS compatibility list
  entry is Adélie 1.0-beta1 on **ppc64** (big-endian), needing `easy-kernel-power8` rather than
  `easy-kernel`, with KDE 5 stable, reported by awilfox.
  (https://www.adelielinux.org/ `[fetched]`;
  https://wiki.raptorcs.com/wiki/Operating_System_Compatibility_List `[fetched]`)
  **Not verified: current Adélie release/version and whether ppc64le is a first-class target.**
- **ArchPOWER** — https://archlinuxpower.org/ , unofficial Arch port. Supports `powerpc64le`
  (>=POWER8), `powerpc` (>=604), `powerpc64` (>=POWER4+/G5 and PS3), and `espresso` (Wii U SMP).
  Has `base` and `testing` repos, installer ISOs, and a Discord. `[fetched]`
- Others listed as working on the RCS OS compatibility list (reporter-attributed, not verified
  here): AlmaLinux 8.x/9.x, CentOS/CentOS Stream, Devuan 4.0, Bedrock, BonSlack, plus many "not
  tested yet" entries. (https://wiki.raptorcs.com/wiki/Operating_System_Compatibility_List)
  `[fetched]` The list is community-maintained and uneven in freshness.

### BSDs

- **FreeBSD** — the official platforms page (last modified 2026-05-08) still lists `powerpc64`
  (big-endian) and `powerpc64le` as **Tier 2** for 14.x, 15.x *and* projected 16.x. 32-bit
  `powerpc` and `powerpcspe` are Tier 2 for 14.x only. (https://www.freebsd.org/platforms/)
  `[fetched]`
  However, Talospace reported on 2025-11-21 that FreeBSD is "considering retiring powerpc64 prior
  to branching 16, which would make FreeBSD 15 the last stable version to support the architecture",
  and noted the proposal's wording says "powerpc64 and powerpc64le" but then only argues about the
  big-endian port. Cameron Kaiser asked Warner Losh for clarification and had not received a reply
  at time of writing. (https://www.talospace.com/2025/11/freebsd-considering-end-of-ppc64-support.html)
  `[fetched]`
  **Open question / conflict:** the platforms page and the retirement proposal disagree. As of
  today the official page still shows Tier 2 through 16.x. Do not state FreeBSD/powerpc64 is
  dropped.
- **OpenBSD** — `powerpc64` runs on PowerNV machines with POWER9. The port page states it "runs
  stably on PowerNV machines based on the Raptor Computing Systems Talos II and Blackbird boards",
  POWER8 support is included but untested, and it **does not run under a hypervisor** (no PowerVM,
  no PowerKVM). First official release was OpenBSD 6.8; latest supported release is **OpenBSD 7.9**
  with miniroot79.img. Maintainer Mark Kettenis; mailing list ppc@openbsd.org. Install media boots
  from the Petitboot menu. (https://www.openbsd.org/powerpc64.html) `[fetched]`
  OpenBSD's ports tree even packages Raptor firmware: `sysutils/talos-ii-pnor-bootkernel-2.10`,
  "BOOTKERNEL partition extracted from the Talos II PNOR firmware bundle published by Raptor".
  (https://openports.pl/path/sysutils/talos-ii-pnor-bootkernel) `[snippet]`
- **NetBSD** — the ports page tiers `evbppc` (PowerPC evaluation boards) as **Tier I**, and
  `amigappc`, `bebox` and other PowerPC platforms as Tier II, latest release 11.0. There is **no
  POWER9/PowerNV port**; Talospace states outright "there isn't a NetBSD port that runs on POWER9".
  (https://www.netbsd.org/ports/ `[fetched]`;
  https://www.talospace.com/2025/11/freebsd-considering-end-of-ppc64-support.html `[fetched]`)

### Toolchains and language runtimes

- **Rust** — `powerpc64le-unknown-linux-gnu` is **Tier 2 with host tools** ("PPC64LE Linux, kernel
  3.10+, glibc 2.17"), i.e. std and rustc/cargo build for it and builds are automated, but tests
  are not always run. Also Tier 2 with host tools: `powerpc64-unknown-linux-gnu` (big-endian),
  `powerpc64le-unknown-linux-musl`, `powerpc64-unknown-linux-musl`, `powerpc-unknown-linux-gnu`.
  No PowerPC target is Tier 1. (https://doc.rust-lang.org/rustc/platform-support.html) `[fetched]`
- **Go** — the gc compiler supports `ppc64` and `ppc64le` ("the 64-bit PowerPC instruction set,
  big- and little-endian"). Neither is a **first class port**: the first-class list is only
  darwin/amd64, darwin/arm64, linux/386, linux/amd64, linux/arm, linux/arm64, windows/386,
  windows/amd64. Consequences per policy: broken builds on secondary ports do not block releases,
  and port maintainers are responsible for keeping them working.
  (https://go.dev/doc/install/source , https://go.dev/wiki/PortingPolicy) `[fetched]`
- **LLVM/Clang** — PowerPC is a long-standing in-tree backend and is what Chimera Linux is built
  with end to end. Raptor also publishes rebuilt `llvm-toolchain-16/18/19` Debian packages in its
  PPA. **Not verified: I did not read an LLVM support-tier statement for PowerPC; do not claim a
  tier.** (https://quickbuild.io/~raptor-engineering-public/+archive/ubuntu/chromium) `[snippet]`

### Browsers

- **Chromium on ppc64le — the patch set has moved, and it is actively maintained.** The
  authoritative patch set is now
  `https://gitlab.raptorengineering.com/raptor-engineering-public/chromium/openpower-patches`.
  Gentoo's ebuild explicitly switched `SRC_URI` from
  `gitlab.solidsilicon.io/public-development/open-source/chromium/openpower-patches` to the Raptor
  Engineering GitLab, and keyworded Chromium `~ppc64`, adding 4K-page-size support and an
  ISA-3.0/POWER9 build gated on `cpu_flags_ppc_vsx3`. Given Solid Silicon's domains no longer
  resolve, the Raptor URL is the only working one.
  (Gentoo commit: https://archives-cdn-origin.gentoo.org/gentoo-commits/1737014417.908da804d868b8eef3b2c2680adacad48155da39.kangie@gentoo/)
  `[snippet]`
- Raptor's own Debian/Ubuntu builds: https://quickbuild.io/~raptor-engineering-public/+archive/ubuntu/chromium
  — "Ungoogled Debian packages regularly rebuilt from Debian security releases". Most recent
  uploads listed: `chromium 151.0.7922.169-1raptor0~deb13u1` (2026-08-21) and `~deb12u1`
  (2026-08-20), both by Timothy Pearson. Ten days old at time of writing — this is live, current
  work. `[snippet]`
- Distribution status: Chromium is in the Debian and Fedora (41+) repos for ppc64el. Fedora adopted
  the shared patch set in July 2024 (Than Ngo at Red Hat). Debian's patches are largely the same as
  Raptor's; Gentoo's are applied on top of Raptor's.
  (https://wiki.raptorcs.com/wiki/Porting/Chromium , https://forums.raptorcs.com/index.php?topic=501.0)
  `[snippet]`
- Upstreaming: Timothy Pearson opened a chromium-dev thread asking about merging baseline ppc64el
  patches, noting years of repeated downstream rebasing across Raptor, Debian, Gentoo and Void. The
  response pointed at Chromium's new-port policy ("Code in the Chromium project should be in
  service of other code in the Chromium project") — effectively a no.
  (https://groups.google.com/a/chromium.org/g/chromium-dev/c/z5qbhoV-fNU) `[snippet]`
- The older https://github.com/shawnanastasio/chromium_power patch framework is described on the
  RCS wiki as unmaintained; do not point people at it as the primary path. `[snippet]`
- **Firefox / SpiderMonkey JIT on Power ISA — this is the biggest 2026 development.** Cameron
  Kaiser's original ppc64le JIT (Bugzilla 1860412, filed 2023) had stalled; Kaiser wrote in that
  bug that a critical wasm fault plus a change in his employment had left it unusable, and OSNews
  covered the stall on 2026-01-07.
  (https://bugzilla.mozilla.org/show_bug.cgi?id=1860412 ,
  https://www.osnews.com/story/144144/firefox-on-power9-the-jit-of-it/) `[snippet]`
  Trung Le (runlevel5) then revived and greatly extended it. Per the Mozilla Discourse post dated
  2026-07-07 and the tracking PRs: ESR 153 and current trunk support, full WebAssembly, WASM SIMD,
  JSPI, POWER10 optimisations, and an ARM64-host PPC64 simulator so CI can run without POWER
  hardware. Big-endian ppc64 is supported too, on both ELFv1 and ELFv2, and there is a
  PowerPC 970 (G4/G5) patch. Claimed results: 13,715/0 jit-tests and full jstests passing on real
  POWER9 (Blackbird), real POWER10 and real little-endian POWER8 hardware, plus all three simulator
  configurations. Supported by Raptor Computing Systems, Oregon State University and the PPC Linux
  community.
  (https://discourse.mozilla.org/t/spidermonkey-jit-with-full-wasm-support-for-power-isa-ppc64le/148860 ,
  https://github.com/runlevel5/firefox-ppc64/pull/1 ,
  https://github.com/runlevel5/firefox-ppc64/pull/2) `[snippet]`
  Downstream patches are published as release tarballs, most recently
  `FIREFOX_153_0_4_RELEASE_PPC64` published **2026-08-11**, containing eight patches (VSX for Skia
  and libwebp LE+BE, LE and BE JIT backends, big-endian GHASH in NSS, BE Skia rendering, and the
  PowerPC 970 JIT).
  (https://github.com/runlevel5/firefox-ppc64/releases/tag/FIREFOX_153_0_4_RELEASE_PPC64) `[snippet]`
  Prebuilt Fedora 44 packages: https://copr.fedorainfracloud.org/coprs/sharkcz/talos/ `[snippet]`
  Upstreaming status: **not merged.** Mozilla asked for the SpiderMonkey port-proposal
  questionnaire (https://spidermonkey.dev/port-proposal-template) and for the `jit/ppc64/` changes
  to be split from the shared-file changes before deciding. The Fedora Firefox maintainer (Dan
  Horák) has said he intends to ship the JIT in the Fedora package.
  (https://bugzilla.mozilla.org/show_bug.cgi?id=1860412 , https://forums.raptorcs.com/index.php?topic=650.30)
  `[snippet]`
- **V8 on Power** — IBM has historically maintained ppc64le support in V8/Node. **Not verified
  here; I did not check the current state of the V8 ppc64 port.**

### Other porting notes

- The RCS wiki maintains a "Fixes in Progress" upstreaming tracker with Mesa, kernel, multimedia
  (FFmpeg, gstreamer/orc ELFv2), Firefox and distro entries. Its Firefox section currently reads
  "[IN PROGRESS] PPC64 JIT backend". Several entries date to the 4.18/4.20 kernel era, so the page
  is partly historical. (https://wiki.raptorcs.com/wiki/Fixes_in_Progress) `[fetched]`
- Recurring cross-cutting problems named by multiple sources: Mesa regressions on big-endian
  (mesa-amber workaround), 64K vs 4K page size assumptions in upstream software, ELFv1 vs ELFv2 ABI
  splits on big-endian, and poor low-level debugging tooling on ppc64le (Kaiser described stepping
  through thousands of instructions in gdb; he mused about needing something like `rr` for
  ppc64le). (https://chimera-linux.org/news/2026/03/retiring-powerpc.html `[fetched]`;
  https://www.osnews.com/story/144144/firefox-on-power9-the-jit-of-it/ `[snippet]`;
  https://www.talospace.com/2024/06/chromium-power-isa-patches-from-solid.html `[snippet]`)

## 6. Name and domain collisions

- **openpower.tools itself is a Namecheap parking page. Confirmed.** DNS: apex `A` record
  192.64.119.150; `www` is a CNAME to `parkingpage.namecheap.com.` then
  `parking.d.parity.domains.`; nameservers `dns1.registrar-servers.com.` /
  `dns2.registrar-servers.com.` (Namecheap). `[dns]`
  HTTP: `http://openpower.tools/` returns 200 and redirects to `http://www.openpower.tools/`,
  serving a 2,963-byte Namecheap lander that links to
  `https://www.namecheap.com/?utm_source=parkingpage...` and loads
  `https://lander.parity.domains/js/...`; the body text reads "has been recently registered with".
  `[http]`
  **HTTPS does not work**: `https://openpower.tools/` timed out (connection timeout after 25s), and
  Exa's crawler also failed on it with `CRAWL_LIVECRAWL_TIMEOUT`. `[http]` This is expected for a
  parking page and will be fixed by GitHub Pages issuing a certificate, but it does mean any
  existing inbound `https://` links are currently broken.
  WHOIS via the standard client returned "TLD is not supported" from the Identity Digital server,
  so registrant/creation date was **not** verified.
- **No existing project or site called "openpower.tools" was found.** Searches for the exact string
  and for "openpower tools" surfaced no competing site. `[snippet]`
- **Soft collision worth knowing about: "OpenPOWER Developer Tools."** The OpenPOWER Foundation
  itself launched a resource under that name — "Introducing OpenPOWER Developer Tools – A One Stop
  Resource for Porting and Building OpenPOWER Compatible Solutions", described as available in the
  Technology Resources section of the OpenPOWER website.
  (https://openpowerfoundation.org/blog/introducing-openpower-developer-tools/) `[snippet]`
  The blog post is old (it references "a membership over 90 members", so roughly 2015) and
  https://openpowerfoundation.org/technology-resources/ returned `CRAWL_NOT_FOUND` today, so the
  resource may no longer exist at that path. Still, the *name* overlaps and OPF is a Linux
  Foundation project with an obvious trademark interest in "OpenPOWER". This is the main naming
  risk to think about.
- **Other similar-sounding things that are not us:**
  - https://github.com/open-power-sdk — "Software Development Toolkit and Libraries for Power",
    IBM-associated, 18 public repos, blog at https://developer.ibm.com/linuxonpower/sdk/ . `[snippet]`
  - https://github.com/open-power/op-image-tools — "image build tools for open-power". `[snippet]`
  - https://github.com/open-power/op-test — the OpenPower Test Framework. `[snippet]`
  - https://github.com/open-power-ref-design-toolkit — an org whose repos last saw activity
    2017-2021. `[snippet]`
  - https://www.opentools.page/ — unrelated AI SDK, no POWER connection. `[snippet]`
- Practical implication: "OpenPOWER" is a Linux Foundation / OpenPOWER Foundation mark. A
  community site at openpower.tools should probably carry a plain disclaimer that it is
  independent and not affiliated with or endorsed by the OpenPOWER Foundation, IBM, or Raptor
  Computing Systems, and should avoid the OPF logo. **Not verified: the exact trademark
  registration status or usage policy for "OpenPOWER" — that should be checked before launch.**

---

## Open questions / not verified

1. Did the rumoured Q1 2026 Raptor product announcement ever happen? Nothing was found on
   raptorcs.com, the forums, Talospace or Phoronix. The rumour source is a single anonymous tip
   appended to a Talospace post.
2. What is Raptor's actual current state as a going concern? Every SKU is "Out of Stock (Special
   Order)", the last firmware release was February 2024, git.raptorcs.com is years idle, and
   gitlab.raptorengineering.com was throwing 502s today. These are individually explainable but
   collectively worth a careful, non-alarmist note.
3. Is gitlab.raptorengineering.com intermittently down or permanently degraded? It needs re-probing
   over several days before we describe it either way. It is currently the *only* documented source
   for buildable Talos II / Blackbird firmware at the v2.10 level.
4. Is Solid Silicon still a going concern? Their domains have no DNS at all, but Talospace reported
   they were listed as an OPF Platinum member as of mid-2025. Check the OPF members page directly.
5. Did Chimera Linux actually remove big-endian ppc64 and 32-bit ppc in July 2026? No follow-up news
   post exists; the repos would need to be checked directly.
6. Did FreeBSD proceed with retiring powerpc64? The platforms page (updated 2026-05-08) still shows
   Tier 2 through projected 16.x, contradicting the November 2025 proposal. Needs a check of the
   freebsd-ports/freebsd-arch list archives and the 16.x branching notes.
7. Current Libre-SOC status. Wikipedia cites a June 2024 message calling it "effectively
   terminated", but the site is up and Open Collective shows 2026 activity. The primary list-serv
   message was not located.
8. Whether A2O has actually reached Power ISA 3.0C/3.1C compliance, and whether the OPF internship
   programme for that ran.
9. Microwatt's exact current ISA compliance level and SMP status — sourced only from a forum post
   quoting linuxppc-dev, not from the repo.
10. V8 / Node.js ppc64le support status was not checked at all.
11. LLVM's formal support tier (if any) for PowerPC.
12. Adélie Linux's current release and whether ppc64le (not just ppc64) is supported.
13. Trademark/usage policy for the "OpenPOWER" name.
14. Registrant and registration date for openpower.tools — the `.tools` WHOIS was not queryable with
    the tooling here.
15. Whether any reproducible-build effort exists for Raptor PNOR/BMC images. None was found.
16. Whether the OpenPOWER Foundation intends to keep `git.openpower.foundation` redirects working
    long-term, given the move to GitHub.
