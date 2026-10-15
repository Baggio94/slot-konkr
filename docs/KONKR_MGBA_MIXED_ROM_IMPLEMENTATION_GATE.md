# mGBA Cable with two different GBA cartridges — implementation gate

Status: **IMPLEMENTED AS EXPERIMENTAL KMLB/4 CANDIDATE ON PR #29, NOT DEVICE-VALIDATED**. Native mGBA dual-cartridge tests pass; real KONKR ↔ Mac mixed-ROM trade and post-Link save integrity remain required QA before release/merge.

## Why the a7 engine deliberately refuses mixed ROMs

The mGBA libretro patch (`cores/mgba/link-mode.patch`) calls `_linkCreatePeer(game,...)` using the **same ROM** supplied to the local libretro core. Both the Host and the Join each simulate a two-GBA lockstep pair, but only one cartridge image exists in each pair. In `crates/slot-android/src/link_net.rs`, KMLB/3 `LabGreeting::accepts` consequently requires identical ROM hashes for backend 1, even when two different Pokémon games belong to a valid original-GBA Cable family. The client rejection is intentional and **must not be removed alone**.

## Current experimental implementation (candidate, not production proof)

- mGBA backend uses KMLB/4; gpSP stays on KMLB/3. No silent downgrade to old mGBA KMLB/3, which required same ROM.
- The TCP greeting checks the **two different** GBA header codes against the Cable compatibility catalogue, and requires matching patched-core source recipe and BIOS. Same-code ROMs still require identical full-file SHA-256.
- After KMLB/4 negotiation and before reporting Ready, the worker exchanges both ROM files via bounded framed packets, validates each full-file SHA-256 and actual GBA header, and stages the peer ROM only under app-private `rom-cache/link-peer-cache/`. **This transmits ROM bytes to the other device on the local network**; it never inserts them into the user's ROM library.
- mGBA's internal `mgba_link_peer_rom` loads the verified cached ROM as its second GBA. The local player keeps their own game, SRAM/RTC, display and inputs. This peer option must be specified before libretro ROM load.
- Initial SLK1 creation uses `link_state::with_local_player` to load only the local player's pre-Link progress into the correct GBA, leaving the other cartridge's fresh boot state intact. The Join then transmits their local saved GBA state; Host combines both and sends the authoritative pair, as in existing lockstep.
- The transfer has 32 MiB retail ROM size caps, per-chunk sequencing, 90-second overall deadline, SHA/header rechecks and an explicit cancellation path. Failure closes the connection and leaves existing solo SRAM untouched.
- For different ROMs, **both Android users must explicitly tap Allow once** before the ROM image transfer. Each side waits for the other's approved `R4OK` barrier; the same-ROM mGBA path and gpSP KMLB/3 have no new dialog. Cancellation or permission timeout aborts without sending game image bytes.
- Native tests include a real TCP exchange between two different synthetic ROMs and both mGBA role directions; tests are **not evidence of successful Pokémon trading on real Android devices**.

**Safety / UX note:** The ROM-file exchange is materially different from ordinary game-input-only local multiplayer. An Android one-time permission dialog now discloses the full ROM transfer and requires approval on **both** devices. The transfer is on the local network and is **not encrypted**; users should connect only trusted devices on a trusted LAN. Real UI usability and network performance still require device QA.

## Implementation gates and physical QA

1. **Mutually authenticate actual cartridge identities**: both four-byte GBA header codes, SHA-256 of full ROM, mGBA patched core source recipe, BIOS, and Cable mode. KMLB/4 or a compatible explicit capability negotiation; no fallback to old KMLB/3 with mixed ROMs. Keep exact SHA required for same-code ROMs until revision-specific interop is validated.
2. **Stage the remote player's ROM without affecting local user ROM/saves**: transfer only between mutually accepted peers, off the GL thread, over bounded TCP (max original retail 32 MiB), with limits, deadline, per-chunk progress and SHA-256/header revalidation. Private cache file, atomic publish; never stage to the user's ROM library, never trust an unverified blob or path. Be explicit that this transfers ROM data over the local network and consider privacy/ownership.
3. **Create two distinct GBA cores per device**: expose an internal `mgba_link_peer_rom` option to mGBA; `_linkCreatePeer` loads peer cartridge from the verified private file, with local `core` still bound to Host player 0 or Join player 1. Preserve patch's video/audio/memory callbacks for the LOCAL player only. Change the portable core recipe when patch changes.
4. **Never clone a save state from one game into the OTHER cartridge**: `open_internal` currently uses `link_state::pair(solo)`. Replace that path for mixed ROMs with the incoming role's solo state in its OWN slot, and a fresh validated peer-cartridge state for the other slot. On wire, each role sends its own local GBA snapshot; the Host builds the final SLK1 Host + Join pair from their distinct local states. On both sides verify the two games/BIOS/battery data remained correctly associated; do not deserialize FireRed into LeafGreen.
5. **Solo recovery and save promotion**: `End Link` must extract the correct half and restore only the local ROM and local SRAM/RTC, preserving previous state backups. Failure must retain private snapshots and refuse unsafe write. Independent of the optional Share Saves ON/OFF feature (which ONLY concerns the *same* ROM across mGBA/gpSP).
6. **Test before enabling**: deterministic full-file/hash and truncation/malicious-input tests, two-ROM mGBA link-state tests in native CI, both Host/Join role order, physical KONKR ↔ Mac AVD Pokémon FR/LG Cable trade, same-ROM regression with Final Fight One, remote loss/pause/end and save-state continuity, region/revision mismatches, different ROM sizes, restart and state history integrity. Both devices must use the same authenticated protocol version.

KMLB/4 now uses backend=1 `compatible_cross_title` **only with the full authenticated peer-ROM exchange completing first**. Any failure aborts before Link READY. Device QA and SRAM persistence are still prerequisites for calling this *verified* or merging. gpSP FR/LG Cable and RFU remain available in parallel.

## Related A8 work

- Library's menu cursor row count corrected from five to six
- B returns according to Multiplayer origin (Select+Home versus Game Menu)
- gpSP prefers Cable where both real gpSP serial backends exist
- Share Saves ON previous auto-state preservation via unique generations; conditional dialog on actual core changes with an existing destination auto-state
- Still needed: explicit SRAM conflict source choice (mGBA / gpSP / Cancel) with transaction/rollback; deterministic SRAM corruption analysis; real RFU timing diagnostics; full device QA
