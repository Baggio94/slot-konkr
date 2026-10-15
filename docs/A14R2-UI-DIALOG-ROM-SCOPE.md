# A14r2 — Native Slot UI, modal controls, scoped ROM search

Base: A14r1, code 89. The multiplayer and save-state behaviors of A14r1
remain unchanged. No automatic save export or destructive ROM operations.

## 1. Native header parity
- Apps and RetroAchievements headings are now EXACT PNG textures from the
  Rust slot-ui word_face generator, with alpha 0.68.
- Both placed at (32, 33) in the 720×480 Slot coordinate system, with a
  24px-tall native heading, using screen-width scaling.
- Android-only Set as Home, RA statistics, shortcuts and controller logic
  remain available.

## 2. Shared Slot dialogs
- Two old system AlertDialogs (conflicting shared SRAM and core-switch
  save-state) use SlotModalDialog.
- Dialogs use the same flat dark background, native font and selected-row
  keyboard/controller menu. D-pad up/down navigates, left/right navigates
  actions, A selects, B cancels. Touch remains available.
- The explicit CANCEL choice is focused first for both save conflicts and
  multiplayer permission. A cannot accidentally pick a destructive choice.
- All existing SlotModalDialog users (manual IP, ROM trust, RA sign-in and
  sign-out) share the new focus and navigation behavior.
- Texts shortened without changing security/backup semantics. Android
  system/IME/launcher role permissions continue using Android's own UI.

## 3. ROM scanner
- If the user-selected top-level SAF directory contains ALL THREE folders
  gba, gb, gbc (case insensitive), only those branches and their subfolders
  are scanned; root and unrelated systems are not traversed.
- If any one is missing, the scanner falls back to the full selected tree
  to preserve alternative folder organizations.
- Missing SAF cursors and truncated scans keep the existing cache.
- Incremental metadata/header reuse, quiet-on-unchanged, and deferral of
  changed library publication while playing remain as in A14r1.

QA before promotion: physical controller D-pad + A/B tests on save
conflict, RA login and sign-out; boot scan with all 3 folders; custom
folder fallback; Mac AVD and KONKR boot timing.
