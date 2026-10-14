# mGBA Cable with two different GBA cartridges — implementation gate

Status: **NOT IMPLEMENTED**, despite Pokémon FR/LG/E compatibility on original GBA and confirmed gpSP Cable exchanges.

## Why the a7 engine deliberately refuses mixed ROMs

The mGBA libretro patch (`cores/mgba/link-mode.patch`) calls `_linkCreatePeer(game,...)` using the **same ROM** supplied to the local libretro core. Both the Host and the Join each simulate a two-GBA lockstep pair, but only one cartridge image exists in each pair. In `crates/slot-android/src/link_net.rs`, KMLB/3 `LabGreeting::accepts` consequently requires identical ROM hashes for backend 1, even when two different Pokémon games belong to a valid original-GBA Cable family. The client rejection is intentional and **must not be removed alone**.

## Correct implementation, before exposing mixed ROM as LINKED

1. **Mutually authenticate actual cartridge identities**: both four-byte GBA header codes, SHA-256 of full ROM, mGBA patched core source recipe, BIOS, and Cable mode. KMLB/4 or a compatible explicit capability negotiation; no fallback to old KMLB/3 with mixed ROMs. Keep exact SHA required for same-code ROMs until revision-specific interop is validated.
2. **Stage the remote player's ROM without affecting local user ROM/saves**: transfer only between mutually accepted peers, off the GL thread, over bounded TCP (max original retail 32 MiB), with limits, deadline, per-chunk progress and SHA-256/header revalidation. Private cache file, atomic publish; never stage to the user's ROM library, never trust an unverified blob or path. Be explicit that this transfers ROM data over the local network and consider privacy/ownership.
3. **Create two distinct GBA cores per device**: expose an internal `mgba_link_peer_rom` option to mGBA; `_linkCreatePeer` loads peer cartridge from the verified private file, with local `core` still bound to Host player 0 or Join player 1. Preserve patch's video/audio/memory callbacks for the LOCAL player only. Change the portable core recipe when patch changes.
4. **Never clone a save state from one game into the OTHER cartridge**: `open_internal` currently uses `link_state::pair(solo)`. Replace that path for mixed ROMs with the incoming role's solo state in its OWN slot, and a fresh validated peer-cartridge state for the other slot. On wire, each role sends its own local GBA snapshot; the Host builds the final SLK1 Host + Join pair from their distinct local states. On both sides verify the two games/BIOS/battery data remained correctly associated; do not deserialize FireRed into LeafGreen.
5. **Solo recovery and save promotion**: `End Link` must extract the correct half and restore only the local ROM and local SRAM/RTC, preserving previous state backups. Failure must retain private snapshots and refuse unsafe write. Independent of the optional Share Saves ON/OFF feature (which ONLY concerns the *same* ROM across mGBA/gpSP).
6. **Test before enabling**: deterministic full-file/hash and truncation/malicious-input tests, two-ROM mGBA link-state tests in native CI, both Host/Join role order, physical KONKR ↔ Mac AVD Pokémon FR/LG Cable trade, same-ROM regression with Final Fight One, remote loss/pause/end and save-state continuity, region/revision mismatches, different ROM sizes, restart and state history integrity. Both devices must use the same authenticated protocol version.

Do not claim cross-ROM mGBA compatibility or enable backend=1 `compatible_cross_title` until ALL these gates pass. gpSP FR/LG Cable and RFU may remain available in parallel.

## Related A8 work

- Library's menu cursor row count corrected from five to six
- B returns according to Multiplayer origin (Select+Home versus Game Menu)
- gpSP prefers Cable where both real gpSP serial backends exist
- Share Saves ON previous auto-state preservation via unique generations; conditional dialog on actual core changes with an existing destination auto-state
- Still needed: explicit SRAM conflict source choice (mGBA / gpSP / Cancel) with transaction/rollback; deterministic SRAM corruption analysis; real RFU timing diagnostics; full device QA
