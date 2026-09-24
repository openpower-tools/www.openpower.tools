#!/bin/sh
# Hold the component library to what its cache key assumes: each crate in
# $COMPONENT_CRATES is keyed on its own tree alone (cache-keys.sh), which is
# sound only while none of them has a path dependency (normal, dev or build)
# and no include_str!/include_bytes! reaches outside its own directory. A crate
# that starts reading the site's content belongs with the site, or the key
# must grow to cover what it reads.
set -eu
status=0

deps=$(cargo metadata --format-version 1 --no-deps | jq -r --arg set "$COMPONENT_CRATES" '
  ($set | split(" ")) as $c
  | .packages[] | select(.name | IN($c[])) as $p
  | .dependencies[] | select(.path != null) | "\($p.name) -> \(.name)"')
if [ -n "$deps" ]; then
	echo "$deps" | while read -r edge; do echo "::error::component crate has a path dependency: $edge"; done
	status=1
fi

for crate in $COMPONENT_CRATES; do
	root=$(realpath "crates/$crate")
	outside=$(grep -rnoE 'include_(str|bytes)!\("[^"]+"' "crates/$crate" --include='*.rs' |
		while IFS=: read -r file line match; do
			rel=${match#*\"}
			target=$(realpath -m "$(dirname "$file")/${rel%\"}")
			case "$target" in "$root"/*) ;; *) echo "::error file=$file,line=$line::reads $target, outside crates/$crate" ;; esac
		done)
	if [ -n "$outside" ]; then
		echo "$outside"
		status=1
	fi
done

exit "$status"
