# Slot KONKR U1 — Pause emulation phase

This first pause-menu phase is intentionally narrow and is not a public release.

- **Carousel:** Y no longer opens the legacy per-ROM emulation popup; the key
  is reserved for favourites in U2. X remains Cart Studio; SELECT remains the
  native animated core selector. No changes to X/Y Buttons turbo.
- **Global:** START → Settings → Emulation still writes global defaults only.
- **Pause:** Resume / States / Multiplayer / Settings / Save & Eject for solo.
  Active Link/RFU hides States; irreversible post-Link requires an explicit
  safe eject to solo. Every item uses an action enum rather than hardcoded
  indices, including restoring the right Multiplayer selection after B.
- **Pause → Settings → Emulation:** Show Stats / Frame Pacing /
  Run-Ahead / Reset to Global, saved exclusively to the selected ROM and core.
  Pacing modes apply when gameplay resumes; invalidate stale time debt and
  speculative audio but **never reboot a core**. The app's global defaults
  remain intact.
- **Safety:** Neither Frame Pacing nor Run-Ahead can change during a live
  Link session or a stopped Link core requiring a solo relaunch. Rewind and
  states remain unavailable during Link. Flash-save / unsupported speculative
  audio protection remains enforced and saved profile intent is preserved.
- **Scope boundary:** per-ROM Screen and Gameplay overrides are NOT wired yet;
  do not expose them in Pause Settings until each has a proper persisted
  per-ROM/per-core override and the live renderer/core is tested. The next
  phase adds these independently without mutating global settings.

Tests include action-based pause row model, conditional Multiplayer States
hiding and exact menu back navigation. Device testing on Mac AVD and KONKR is
still required. The public RC1 remains untouched.
