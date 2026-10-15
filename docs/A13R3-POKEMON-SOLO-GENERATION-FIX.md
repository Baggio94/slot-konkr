# A13r3 — Fix LeafGreen End Link false unsafe-recovery conflict

KONKR A13r2, 14 Oct 2026, 23:42:14 and 23:43:00:
`Link recovery unsafe: Solo save changed during Link (...mgba.gen-1792007598729624196.retroarch-export.state.auto)`.

**Cause (grounded in source and error)**: at Link start `open_internal` saved
the **legacy** `{id}.mgba.retroarch-export.state.auto` as its baseline,
but rebuilding an ordinary solo core correctly resolved
`{id}.mgba.gen-{nonce}.retroarch-export.state.auto` from the active
`state-generation.active` file. A13r2 compared these different targets and
falsely classified unmodified SOLO as concurrently modified.

**Change:** Link start now resolves and snapshots the *active* export
filename, including protected generation, using the SAME journal validation
as solo startup. It durably stages the exact expected export filename.
Rejoin checks that SOLO is still on that identical generation, and the
unchanged `promote_player_files` CAS compares the content of the correct
file. True generation changes and genuine concurrent file mutations remain
fail-closed. Flash checksum verification and original solo save backups
remain mandatory; no skipping validation or rewriting the current SRAM.

New pure unit tests cover original default filename, actual KONKR protected
generation key, generation changes and malformed keys.

No network, FPS/PCM timing, ROM transfer, core-state protocol or
in-game SRAM format was modified. A13r3 uses versionCode 86. CI passing
is necessary but **not sufficient** to prove LeafGreen trade, Save & Eject,
and End Link on physical KONKR. Retest with archived solo save and Share Saves
OFF. PR #29 stays Draft on the experimental branch.
