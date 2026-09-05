# Palette fitting scripts

The scripts the accessibility and colour report's computed figures come
from, and the output of the two that were re-run to check them.

They were written during the 2026-09-03 survey and lived in a temporary
directory, which the reports cited. That made the reports' central claim,
that every computed figure is reproducible, depend on a directory that
does not survive a reboot. They are kept here instead.

Run them with the dependencies they were run with:

    uv run --with colour-science --with numpy python refit3.py

`refit3.out` and `refit4.out` are the output of the runs quoted in
`../2-accessibility-colour.md`, kept so a later run can be compared
against what the report actually read.

What they are not: the palette the site ships. That was refitted in Rust
afterwards, in `op-colour`'s `fit_series` binary, against floors the
`palette.rs` tests re-derive from the tokens. These are the working
material the decision came out of, not the decision.
