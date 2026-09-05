# openpower-tools: branching, contribution and release strategy - search report

Read-only research pass. Nothing was modified. Date of search: 2026-09-03.

## (a) Authoritative documents found

1. `/media/j/AXM2G2-1/Projects/op9-firmware-rebuild/WORKFLOW.md` (mtime 2026-08-29 03:46)
   The branch model itself: six branch kinds (`upstream/<publisher>/<version>`, `topic/<name>`,
   `integration/<version>`, `dev`, `release/<version>`, `main`), the commit graph, and four git
   invariants to check. This is the most complete single statement of the model.

2. `/media/j/AXM2G2-1/Projects/op9-firmware-rebuild/MIRRORING-PLAN.md` (mtime 2026-08-29 03:43)
   Publishing plan for the mirror organisation: fork-vs-push rule, naming, attribution, and
   (section 8) the same branch model plus sync direction and mirror-refresh mechanics. Marked
   "Draft for review. Nothing below has been executed."

3. `/media/j/AXM2G2-1/Projects/op9-firmware-rebuild/PLAN.md` (mtime 2026-08-29 03:47, 224 KB)
   The master document. Section 4 restates the strategy/branch model, section 5 reproduces the
   org READMEs verbatim, section 9 gives step-by-step commands, section 10 is an explicit
   decided/open ledger (the single best summary of what is settled vs. not).

4. `/media/j/AXM2G2-1/Projects/op9-firmware-rebuild/VENDORING-NOTES.md` (2026-08-29)
   Working note on git-subtree vendoring mechanics (patch handling, provenance of the three
   reconstructed packages). Touches release-artefact concerns but is not branching/release policy.

5. `/media/j/AXM2G2-1/Projects/op9-firmware-rebuild/README.md`
   One paragraph pointing at a `CONTRIBUTING.md` that does not exist in this tree (see gaps).

6. `/media/j/AXM2G2-1/Projects/op9-firmware-rebuild/mirror/publish`, `mirror/plan.sh`,
   `mirror/validate`, `mirror/sources.tsv`
   The executable side. `publish` and `plan.sh` are GitHub-CLI (`gh`) scripts hard-coding
   `MIRROR_ORG='op9-onl-mirrors'` / `WORK_ORG='op9-onl'`. `validate` checks
   integrity/refs/pins/alternates/contains against `sources.tsv`/`pins.tsv` and is
   platform-agnostic (no org names).

7. `/home/j/Projects/op9-firmware-planning/org-readmes.md` (mtime 2026-08-29 01:20)
   Draft prose for both org READMEs, an attribution note, and a provenance statement. Explicitly
   superseded by WORKFLOW.md's branch model per PLAN.md's own text, but still the only full
   README-style version-control explanation written for outside readers.

8. `/home/j/Projects/op9-firmware-planning/CONTENTS.txt` (mtime 2026-08-31)
   Short index recording the 2026-08-30 org rename and the series' commit counts.

9. `/home/j/Projects/op9-firmware-planning/series-revised-report.md` (2026-08-29)
   Summary/counts of the 79-commit series and the seven findings that reshaped it. Not policy
   itself, but explains why the branch model had to change (the override/patch ordering
   constraint: a package with `<PKG>_OVERRIDE_SRCDIR` set never reaches buildroot's patch step,
   so fork branches carrying patches must exist before the override commit).

10. `/media/j/AXM2G2-1/Projects/op9-firmware/CONTRIBUTING.md` (duplicated at
    `/home/j/Projects/op9-firmware/CONTRIBUTING.md`)
    The earlier, unsquashed tree's contribution guide: subtree layout and `git subtree
    pull`/`split` commands only, no org or branch-model content. Superseded by the rebuild tree.

11. `/home/j/.claude/projects/-home-j-Projects-openpower-tools-www-openpower-tools/memory/reference_openpower_strategy_docs.md`
    Not a project document, but the most useful index found: names WORKFLOW.md/MIRRORING-PLAN.md/
    PLAN.md sections 4, 9, 10 / org-readmes.md as "the strategy of record" and flags that all of
    them predate the GitLab rename.

12. `/home/j/.claude/projects/-media-j-AXM2G2-1-Projects-op9-firmware-rebuild/memory/openpower-tools-org-rename.md`
    and `/home/j/.claude/projects/-home-j-Projects-openpower-tools-www-openpower-tools/memory/project_openpower_gitlab_org.md`
    (both 2026-09-03)
    The only records of what actually replaced the GitHub-org plan: a GitLab group. Not reflected
    in any of documents 1-10 above.

Also checked, found to contain no additional branching/contribution/release policy: `/home/j/Projects/openpower`,
`/media/j/AXM2G2-1/Projects/talos-firmware` (build trees with each vendor's own upstream
CONTRIBUTING.md, not project policy), `/home/j/Documents/OpenPOWER` and its `reorg/` subtree (a
hardware-documentation/PDF archive; there is no separate `OpenPOWER-reorg` directory, `reorg/` is
nested inside `OpenPOWER/`), and `/home/j/Projects/openpower-tools/www.openpower.tools` itself
(the community website repo has `docs/development.md` covering only the Rust/Trunk build; no
branching or release policy of its own).

## (b) The decisions, consolidated

### Branching model

Source: `WORKFLOW.md` (full text); restated `MIRRORING-PLAN.md` section 8; confirmed decided in
`PLAN.md` section 10.1.

| branch | holds | lifetime |
|---|---|---|
| `upstream/<publisher>/<version>` | a publisher's tree at that release, pristine, fetched from the mirror | permanent, one per publisher and release |
| `topic/<name>` | one unit of work; its base names the destination | until merged or rejected |
| `integration/<version>` | publishers reconciled, submittable topics merged | until merged to `dev` |
| `dev` | what is being shipped | permanent |
| `release/<version>` | a train, cut from `dev` | permanent |
| `main` | what was released | permanent |

The firmware monorepo itself has no upstream to track, so it only carries `topic/`, `dev`,
`release/<version>`, `main`. "Publishers" exist because some packages have more than one upstream
(`linux`/Raptor vs mainline, `skiboot` vs `raptor-talos-skiboot`, `libflash`, `sb-signing-framework`),
so branch names carry the publisher: `upstream/torvalds/v6.6.16` vs `upstream/raptor/v6.6.16`.

This **replaced an earlier, simpler model**: a single `talos/<version>` branch per fork, described
in `org-readmes.md` and `PLAN.new.md`. `PLAN.md` says so explicitly (section 5): "org-readmes.md
was written against the single talos/<version> branch per fork, which section 4 replaces... The
single talos/<version> branch per fork is gone." A subagent transcript from the session that
regenerated `PLAN.md` confirms this happened as a same-day revision: "The branch model was
replaced. The earlier report describes a single talos/<version> branch per package fork. That
model is gone. MIRRORING-PLAN.md section 8 in the tree now carries the real one, with a commit
graph." (`/home/j/.claude/projects/-home-j-Projects-energia/56e60f0a-a678-404c-bf66-84398e54a958/subagents/agent-a63c862de6e6846d2.jsonl`,
2026-08-28T16:49:28Z)

### Contribution flow

A topic branch's base declares its destination: `git merge-base --is-ancestor
upstream/raptor/<version> <topic>` means it goes to Raptor, `upstream/torvalds/<version>` means
mainline, based on `dev` means it stays local. "A pull request opens from a topic with no
rebasing, because the topic already sits on the tree it is destined for." (`WORKFLOW.md`)

Every commit carries an `Upstream-Status:` trailer (OpenEmbedded's convention). Four values in the
settled model: `Submitted [where]`, `Backport [version]`, `Pending`, `Denied` (`Inappropriate`
from the earlier five-value draft is explicitly retired per `PLAN.md` section 10.1).
`integration/<version>` is where multi-publisher conflicts are resolved exactly once, in a merge
commit whose message records why.

Pull requests to upstream are expected to work "from a fork of a fork" within the same GitHub fork
network, but `MIRRORING-PLAN.md` section 7 flags this as unverified: "Confirm this on the first
package that has something to send upstream, before relying on it for the rest."

### Release strategy

`release/<version>` is cut from `dev` after the previous release has merged back (`git log
dev..main` must be empty before cutting). Shipping is `main <- --no-ff merge <- release/<version>`;
that merge commit is the one thing the firmware monorepo subtrees (`git subtree add --squash`,
pinned to that commit). `main` is then merged back into `dev` so the invariant holds. Hotfixes are
authored on the oldest affected train and merged forward through newer trains, one commit identity
throughout (`git branch --contains <sha>`).

Versioning is only partly settled. An `upstream/<publisher>/<version>` token is unambiguous (the
publisher's own tag: "A version string means the publisher's own tag"), but `PLAN.md` section 10.2
records as explicitly open: "What `<version>` means, now that one token names three branches...
Settle whether a firmware release that carries no upstream bump gets its own `release/` name."

No release cadence is documented anywhere found. No commit/tag signing policy for the project's
own releases was found in any document (the GPG-verification material that exists, e.g. GMP's
reconstruction checked against the GNU keyring, verifies upstream provenance, not our own
releases). At the build/artefact level (not git), a release's identity is `firmware-version.sh`'s
`<stamp>.<commit>.<hash>.<serial>` scheme written into `op-build/customrc`, checked against a
`VERSION` partition inside the PNOR image (source: memory note
`/home/j/.claude/projects/-home-j-Analysis-lm-disk-recovery-2026-05-03-vlm-rebuild/memory/firmware-monorepo.md`).

### Mirror and fork naming policy

As documented, `MIRRORING-PLAN.md` section 3 / `PLAN.md` "Naming": "Upstream's own name,
lowercased, no prefix: `glibc`, `busybox`, `binutils-gdb`. The organisation name already says these
are mirrors, so a `mirror-` prefix would repeat it 83 times." Five buildroot-package-vs-project-name
collisions are resolved explicitly (`mtd`->`mtd-utils`, `nvme`->`nvme-cli`, `loadkeys`->`kbd`,
`squashfs`->`squashfs-tools`, `libargon2`->`phc-winner-argon2`); three more
(`libopenssl`->`openssl`, `libzlib`->`zlib`, `gettext-gnu`->`gettext`) and three
(`ncurses`, `pnv-lpc`/`talos-skiboot`, `linux`/`op-linux`) are recorded as still undecided in
`PLAN.md` section 10.2.

Fifty of 83 sources get GitHub forks (preserves "forked from" attribution and the upstream link);
the other 33 get `gh repo create` + `git push --mirror`; three (`lzo`, `memtester`, `gmp`) are
hand-reconstructed from tarballs and must carry `PROVENANCE.md` in every commit plus a repository
description that leads with the word "reconstruction".

As actually done, this naming policy was not followed. `mirror/sources.tsv` (the tree's own
manifest) and the live GitLab subgroup both use publisher-prefixed names taken from the verus
bare-mirror layout: `raptor-hostboot`, `raptor-skiboot`, `raptor-sbe`,
`openpower-sb-signing-framework`, `raptor-op-build-blackbird`, not the documented "no prefix" rule.
See gaps, item 3.

### GitLab vs GitHub roles

Two things are true simultaneously in the source material, reconciled only by memory notes:

- Every planning document (`WORKFLOW.md`, `MIRRORING-PLAN.md`, `PLAN.md`, `mirror/publish`,
  `mirror/plan.sh`) designs a two-GitHub-org system: `op9-onl-mirrors` (provenance, no commits of
  ours ever) forking/pushing from upstream, and `op9-onl` (the work) forking from the mirrors, with
  the firmware subtreeing from `op9-onl`. Raptor's own components are described as living on
  Raptor's own GitLab (`gitlab.raptorengineering.com`), treated purely as an upstream to mirror
  from, not as infrastructure of ours.
- In reality, per `/home/j/.claude/projects/-media-j-AXM2G2-1-Projects-op9-firmware-rebuild/memory/openpower-tools-org-rename.md`
  (corrected 2026-09-03): "Canonical home: `https://gitlab.com/openpower-tools`, a public group
  created 2026-08-30. Mirrors live in the subgroup `openpower-tools/upstream/<mirror-name>`...
  GitHub `openpower-tools` holds only `www.openpower.tools`... GitHub `openpower-tools-upstream`,
  `op9-onl` and `op9-onl-mirrors` exist and are empty." GitLab is the actual publication target;
  GitHub's only live role is Pages hosting for the community website.

## (c) Transcript findings

- `/home/j/.claude/projects/-media-j-AXM2G2-1-Projects-op9-firmware-rebuild/d79ad476-f368-4d3f-9221-ecd3b8fba115.jsonl`
  (session `d79ad476`, 2026-09-03). The pivotal message, 08:43:04Z: "we're on gitlab openpower-tools
  org, instead of github - we're publishing some things to github just for pages hosting (does
  gitlab offer it?)". This is the correction that produced `openpower-tools-org-rename.md` two
  minutes later (memory note `modified: 2026-09-03T08:45:06Z`). Found by a direct targeted keyword
  search (gitlab/rename/canonical/pages hosting/publish) run outside the primary branching+org-name
  filter, because this exact message does not contain any of the primary keywords.

- `/home/j/.claude/projects/-home-j-Projects-openpower-tools-www-openpower-tools/c60961e2-8b98-4180-bad9-d316c22e47bc.jsonl`
  (session `c60961e2`, this project's own session). 2026-09-03T08:47:35Z, four minutes after the
  message above: "there should be transcripts and other information about openpower-tools and the
  branching/contribution and release strategy, let's find them" - the direct origin of the research
  task this report answers.

- `/home/j/.claude/projects/-home-j-Projects-energia/56e60f0a-a678-404c-bf66-84398e54a958.jsonl`
  (session `56e60f0a`, "energia" project, 2026-08-27/28). This is where the series/mirror plan was
  actually authored (`CONTENTS.txt` says so explicitly: "Written 2026-08-29 in the Projects/energia
  session"). Notable exchanges: 2026-08-28T11:05Z, an agent verifying the squash-and-replay plan
  against a restored 94,383-commit repo ("your plan is safe... I owe you a correction"); 2026-08-28T12:20:28Z,
  a short user aside, "mirrors option should nkt [not] be needed at all" (context for what
  flag/option this refers to was not recovered within the search budget; flagged, not resolved);
  2026-08-28T14:31Z and 15:29Z, agent reports of the GMP reconstruction and the 81-row
  `repo-descriptions.md` completing.

- `/home/j/.claude/projects/-home-j-Projects-energia/56e60f0a-a678-404c-bf66-84398e54a958/subagents/agent-a5b1a0af2e3ceeb71.jsonl`
  and `agent-a63c862de6e6846d2.jsonl` (both 2026-08-28, ~16:22-16:49Z). These are the two subagent
  runs that composed `PLAN.md` from sources. One names `MIRRORING-PLAN.md` as covering "the
  two-organisation mirror and fork strategy, branch management, scope, naming, attribution,
  execution"; the other documents the branch-model replacement quoted in section (b).

- `/home/j/.claude/projects/-media-j-AXM2G2-1-linux/4e8cde3d-be41-4acd-ab8c-780e90c162af.jsonl`
  (session `4e8cde3d`, POWER9 kernel/NX-GZIP work, 2026-09-01/02). Tangential: a separate clone
  (`/media/j/AXM2G2-1/linux`) actively using topic/integration-style branches for kernel
  development, but for a different workstream (fleet NX-GZIP enablement), with an added
  `control`/experimental-host branch shape not in `WORKFLOW.md`'s table. One user aside,
  2026-09-02T07:25:43Z, "and has our firmware fork altered that", references the firmware fork
  without enough surrounding context recovered to attribute a decision.

- `/home/j/.claude/projects/-home-j-Analysis-lm-disk-recovery-2026-05-03-vlm-rebuild/28f501bd-1089-4555-91f7-66c481fb1843.jsonl`
  (session `28f501bd`, 2026-07 through 2026-08-21). This project's memory directory holds several
  genuinely relevant reference notes (org creation date, Raptor GitLab's HTTP 500-on-pack-generation
  behaviour, Software Heritage recovery of Raptor repos), but the raw transcript itself is almost
  entirely a different, unrelated project (a Symbolics Lisp Machine emulator reconstruction); no
  additional branching/release decisions beyond what the memory notes already distil were found
  within the search budget.

- `/home/j/.claude/projects/-home-j-Projects-openpower-tools-www-openpower-tools/c60961e2-8b98-4180-bad9-d316c22e47bc/subagents/agent-a29715d6ce34e3c9f.jsonl`
  and `/home/j/.claude/projects/-media-j-AXM2G2-1-Projects-op9-firmware-rebuild/d79ad476-f368-4d3f-9221-ecd3b8fba115/subagents/agent-acc04f00b4aacb372.jsonl`
  matched the primary grep filter but yielded no user-role text beyond what is captured above.

No `.jsonl` transcripts exist under `/home/j/.claude/projects/-home-j-Documents-OpenPOWER/` or
`-home-j-Documents-OpenPOWER-reorg/` (checked directly; only `memory/` subdirectories are present,
and their content, font/beam-philosophy feedback, a typed-SRAM project, an ISA transcription pilot,
is unrelated to org strategy).

One transcript file, `/home/j/.claude/projects/-home-j-Projects-openpower-tools-www-openpower-tools/c60961e2-8b98-4180-bad9-d316c22e47bc/subagents/agent-a67797385c13746b2.jsonl`,
also matched the primary grep filter but was excluded from analysis: it is this agent's own
in-progress transcript for the present task, not a prior recorded decision.

## (d) Gaps and contradictions

1. The entire GitHub two-org plan is superseded by an undocumented GitLab pivot. `WORKFLOW.md`,
   `MIRRORING-PLAN.md`, `PLAN.md`, and both mirror scripts (`mirror/publish`, `mirror/plan.sh`)
   design and implement (via `gh`) a system of two GitHub organisations. As of 2026-08-30 the
   actual canonical home became the GitLab group `gitlab.com/openpower-tools` with subgroup
   `openpower-tools/upstream`, and 32 mirrors were already pushed there by 2026-09-03, by some
   means outside the repository's own scripts, since `mirror/publish`/`plan.sh` are `gh`-only and
   still name the pre-rename GitHub orgs. No document anywhere restates the branch model (`topic/`,
   `integration/<version>`, `dev`, `release/<version>`, `main`) in GitLab terms, or says whether
   GitLab's group/subgroup/fork mechanics can even carry that model unchanged. This is the central,
   load-bearing gap.

2. `mirror/publish` and `mirror/plan.sh` hard-code the pre-rename org names, confirmed directly:
   `mirror/publish:24-25` sets `MIRROR_ORG = 'op9-onl-mirrors'` / `WORK_ORG = 'op9-onl'`, and
   `mirror/plan.sh` calls `gh api orgs/op9-onl-mirrors`, `gh repo create op9-onl-mirrors/<name>`,
   etc. throughout. This is exactly the discrepancy flagged in
   `openpower-tools-org-rename.md`: "mirror/publish still hard-codes the op9-onl names and is
   entirely gh-based, so it does not describe what was actually done." Neither script was touched
   after the 2026-08-30 rename (mtimes 2026-08-29 04:09/04:10); `mirror/validate` and
   `mirror/sources.tsv` were touched 2026-08-30 but carry no org references at all (they operate on
   local paths only), so they are not stale in the same way.

3. Naming policy contradicts naming practice. `MIRRORING-PLAN.md` section 3 / `PLAN.md` "Naming"
   mandate "upstream's own name, lowercased, no prefix." `mirror/sources.tsv` (e.g. `hostboot push
   /mnt/verus/Projects/repos/raptor-hostboot.git`, `sb-signing-framework push
   .../openpower-sb-signing-framework.git`) and the live GitLab subgroup (`raptor-hostboot`,
   `talos-op-build`, `openpower-ppe42-gcc`) both use publisher-prefixed names instead. No document
   reconciles this; the verus bare-mirror naming convention (older, operational) appears to have
   been carried straight into the GitLab publication step while the written "no prefix" rule was
   designed against a plan that was never executed as written.

4. `README.md` promises a `CONTRIBUTING.md` that does not exist. `/media/j/AXM2G2-1/Projects/op9-firmware-rebuild/README.md`
   says "CONTRIBUTING.md says how [to fetch upstream history]... covers the layout, the subtree
   mechanics, and how to take upstream changes." No `CONTRIBUTING.md` exists at the top level of
   that tree; only the earlier, unsquashed `op9-firmware` tree has one, and it is explicitly called
   superseded by the `reference_openpower_strategy_docs.md` memory note.

5. `org-readmes.md` is stale but still the only full README text. `PLAN.md` section 5 reproduces it
   "verbatim" while noting in the same breath that it names a `talos/<version>` branch that "will
   not exist" and lists a retired `Upstream-Status` value (`Inappropriate`), deferring the actual
   rewrite to section 10.2, which itself never supplies replacement prose, only a note that it "has
   to become" something else.

6. No release cadence, and versioning is self-admittedly ambiguous. `PLAN.md` section 10.2 records
   as open, in the document's own words: "What `<version>` means, now that one token names three
   branches... Settle whether a firmware release that carries no upstream bump gets its own
   `release/` name." No cadence (time- or event-based) is stated anywhere.

7. No signing policy found for the project's own commits, tags, or release artefacts, as distinct
   from verifying upstream's signatures during mirror reconstruction (documented for `gmp` only).

8. The fork-of-a-fork pull-request assumption is explicitly unverified. `MIRRORING-PLAN.md` section
   7: PRs from `op9-onl` are assumed to work against any repo in the same GitHub fork network, but
   the document itself says to "confirm this on the first package that has something to send
   upstream, before relying on it for the rest", and nothing found in this search confirms it
   happened.

9. Ambiguous, unresolved user note. The energia-session aside "mirrors option should nkt be needed
   at all" (2026-08-28T12:20:28Z) could be a decision about dropping some tool's `--mirrors` flag,
   but no surrounding context was recovered within the search budget to say what it decided;
   flagged rather than interpreted.

10. Several provenance facts are marked unresolved in `PLAN.md` section 10.2 and bear on what a
    mirror repository can honestly claim: whether `raptor-talos-skiboot` and `raptor-skiboot` are
    the same upstream under two paths, whether `libflash`'s pinned commit is reachable from its
    declared `SITE`, and the completeness of two Software-Heritage-recovered repositories (`pnor`,
    `machine-xml`) given `gitlab.raptorengineering.com` intermittently returns HTTP 500
    (corroborated independently by the memory note
    `/home/j/.claude/projects/-home-j-Analysis-lm-disk-recovery-2026-05-03-vlm-rebuild/memory/raptor-gitlab-pack-generation-fails.md`).
