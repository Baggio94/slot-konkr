# A13 — F-Zero frame budget and Pokémon mixed-ROM acceptance

A13 is isolated to `feat/konkr-multiplayer-lab`. Same ARM64+AVD APK.
Save backups are mandatory before installing. Share Saves OFF on both; mGBA
Pokémon Run-Ahead OFF (enforced in the game engine). Do not touch the stable RC4.

## First: preauthorize your two test devices for full mixed-cartridge ROM transfer
Different Pokémon cartridges need their ROM bytes transferred privately between
endpoints to run the current two-GBA mGBA Cable backend. A13 **does not silently
authorize this for everyone on the LAN**. Provision the opt-in locally on BOTH
of your own devices *only if you own/trust each endpoint*:
```bash
ADB="$HOME/Library/Android/sdk/platform-tools/adb"
for serial in BW0308N250009576 emulator-5556; do
  "$ADB" -s "$serial" shell am force-stop fyi.slot.konkr
  "$ADB" -s "$serial" shell run-as fyi.slot.konkr touch files/a13-trusted-mixed-rom.flag
done
```
For untrusted/other devices, remove this marker on BOTH peers (same command,
substitute `rm -f` for `touch`); the permission dialog returns. Pair only
on a private LAN. Only recognized compatible games and cryptographically
matched core/BIOS versions pass. No ROM is inserted into the normal library.

## Pokémon FireRed ↔ LeafGreen protocol
1. Back up the **validated LeafGreen solo save** and FireRed from BOTH private
   Slot Saves folders to an independent Mac tar before attempting Link.
2. Start each game separately in normal solo mode; confirm actual save and
   Continue after a **full eject/restart** with AutoState bypass if needed.
3. Activate mixed-ROM opt-in above. Open Multiplayer Host on one device,
   Join on the other, mGBA Cable. This should not show permission/toast.
4. Complete one in-game trade. Save *inside* each Pokémon game only after
   successful trade; leave the native Link connected for this action.
5. Use End Link on one side. Both devices should detach into own local
   single-GBA session without eject and with their own correct cartridge.
6. Save in-game again, Save & Eject, and cold reopen **without stale state
   injection**. Check Pokémon, party and game progression separately.
7. Compare SHA-256 and Pokémon FLASH1M sector checksums for both SRAM files.
   Any invalid complete bank **must refuse** pre-Link seed, End Link promotion
   or disconnected peer promotion; original solo file remains untouched.
8. Test peer-disconnect while the pause overlay is open; confirm no trap.
9. When testing F-Zero, capture A12 diagnostics. A13 Link display log now
   adds `a13_budget_deferred`. Compare FPS, >25ms callback gaps and audio
   underruns against A11 recovery (A12 run from 14 Oct 2026).

## A13 safeguards/limits
- A11 bounded frame-debt remains. Only a second real mGBA Cable frame
  is gated on first-frame observed cost and PCM queue (20/23ms soft budget).
- A13 does NOT modify the actual 3-frame Link input delay, ROM pair
  compatibility catalogue, portable negotiated ABI fingerprints, or
  serialized SLK1 wire format.
- An invalid Pokémon battery is now rejected before Link and before
  end-of-Link promotion. This is **fail closed**, not automatic Flash repair.
- Existing system UI messages for genuine errors are NOT removed.
  Ordinary End Link/peer EOF no longer overlays an extra success/error toast.
- A successful CI run proves compilation/tests only. Real trades, saved
  progress and smoothness require KONKR + AVD verification.
