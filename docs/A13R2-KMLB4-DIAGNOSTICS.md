# A13r2 — KMLB/4 FireRed ↔ LeafGreen negotiation diagnostics

Physical KONKR + Mac AVD traces (14 Oct 2026, ~23:06–23:10):
- Mac discovers the KONKR Host under Pokémon family `PK3A`.
- Both peers explicitly preauthorize a private KMLB/4 ROM transfer.
- TCP readers start after the Lab greeting and then receive EOF/reset
  before any real multiplayer emulation begins.
- Several attempts fail around the previous 8-second ROM packet timeout;
  this is a plausible cause, **not yet proven**.

A13r2 retains A13r1's restored A12/A11 frame pacing, Pokémon FLASH
safeguards, Link-to-solo transaction model and consent preauthorization.

This version adds **metadata-only** log lines:
`KMLB4 ROM exchange: stage=start|consent|manifest|fragment|verification|ready`;
`KMLB4 setup failure:` includes the underlying network error before it is
reduced to a UI category. Does not log ROM contents, SRAM or hashes.

Transfers remain bounded. Per-fragment wait 8→20s; full exchange 90→180s.
The existing KMLB/4 packet format, 60 KiB chunks, SHA-256 validation and
private cache are unchanged. A native test now transfers two synthetic
16 MiB GBA images (previously only 256/384 KiB). This does not prove
actual Pokémon trade behavior.

To test: install code85 A13r2 on both peers, confirm Share Saves OFF and
Mac backup of good Pokémon saves, then retry mixed-ROM mGBA Cable.
Collect `scripts/slot-kpa-a13r2-pokemon-diagnostics.sh` output if the
connection does not complete. Do not merge to stable without field tests.
