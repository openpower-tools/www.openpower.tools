#!/bin/sh
# Content keys for CI. Each key names exactly the tracked content a stage
# reads, taken from git's own object ids, so a stage whose inputs have not
# changed is restored from the cache instead of run again. Writes the keys to
# $GITHUB_OUTPUT (or stdout when run by hand).
#
#   components  the component library crates (op-webc, op-parts, op-chart,
#            op-colour, op-terms): none depends on another workspace crate or
#            reads a file outside its own directory, so each is its tree.
#   source   what the site's lints, tests and build read: every tracked
#            file except the prose (docs/, README.md), the report tool (tools/)
#            and CI configuration. The crates reach well outside crates/ with
#            include_str! (index.html, styles/, data/, pages/), so the key is
#            everything minus a short list of what nothing compiles in.
#   passes   the source-side part of what the interaction report and its
#            second pass read: the tool and the contract. The workflow adds
#            the built side by content (the digest of the pages the passes
#            load, and of the machine table and comparison bins the site job
#            builds), so the key is everything the passes read and nothing
#            else.
#
# The workflow file, these scripts and the local actions are in every key:
# they hold the commands and pinned versions each stage runs with.
set -eu
blob() { git rev-parse "HEAD:$1"; }
digest() { sha256sum | cut -c1-64; }
ci=$(for f in .github/workflows/deploy.yml .github/scripts .github/actions; do blob "$f"; done)
toolchain="$(blob rust-toolchain.toml) $(blob Cargo.lock)"

components=$({
	for crate in $COMPONENT_CRATES; do blob "crates/$crate"; done
	echo "$toolchain $ci"
} | digest)
source=$({
	git ls-files -s -- . ':!docs' ':!tools' ':!README.md' ':!.github'
	echo "$ci"
} | digest)
passes=$({
	blob tools/interaction_report
	blob data/interaction-contract.json
	echo "$ci"
} | digest)

out=${GITHUB_OUTPUT:-/dev/stdout}
{
	echo "components=$components"
	echo "source=$source"
	echo "passes=$passes"
} >>"$out"
