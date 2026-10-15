# A14r1 — Slot UI finishing & seamless library updates

Experimental branch only. Version code 88, A14r1; A14 is still field-tested
baseline until this cumulative build passes on the KONKR and Mac AVD.

- Apps / RetroAchievements use a common muted Slot Settings heading and
  matching no-toolbar, flat dark opening palette, without altering focus,
  existing icons, RA sync or navigation.
- Library calls the option Share mGBA/gpSP Saves, with right-aligned ON/OFF
  and DPAD LEFT / RIGHT plus A to toggle in place. Preference remains OFF
  by default and only changes shared SRAM behavior at next ROM launch.
- Verified cached ROM index is restored immediately as before. A full SAF
  directory metadata check runs quietly on the scanner worker behind the
  bootlogo. It NEVER delays hiding the bootlogo.
- SAF changed-index comparison ignores provider traversal order; it detects
  added, removed and modified ROM metadata, reuses known cartridge headers
  and publishes only genuine changed indexes. Unchanged index never calls
  nativeSetLibrary, recreates textures or shows a progress indicator.
- A missing SAF cursor and incomplete/truncated enumeration retain the old
  shelf. The previous cache is never overwritten by a partial scan.
- Changed indexes are staged until Rust reports idle and the renderer
  independently checks no game, no inserted cart, no outstanding launch.
  A game started during scanning remains running without ejection.
- A subtle upper-right Slot status spinner shows only while a changed index
  is applied. AtomicFile preserves the previous cache on interrupted writes.
- Manual Refresh Library remains functional with an already-up-to-date notice.
- Tests: stable index order, add/remove/modify, duplicate URI handling,
  native no-eject guard, CI gameplay and Multiplayer non-regression.

Physical acceptance remains: cold-start timing, add/remove ROM, safe refresh
while game inserted, visual header alignment and Library toggle controls.
