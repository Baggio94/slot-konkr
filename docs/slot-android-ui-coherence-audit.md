# Slot for KONKR — Android UI coherence audit
11 October 2026 · PR #4 (DRAFT)

## Audit baseline

The canonical design source is the upstream Slot UI, not Android Material.
`crates/slot-ui/assets/label.ttf` is the original OFL label typeface.
`crates/slot-ui/src/plate.rs`, `text.rs`, `quick_menu.rs` and
`crates/slot-android/src/runtime.rs` provide existing scale, typography,
navigation and colors; keep native rendering untouched.

Shared Android primitives now live in
`android/app/src/main/java/fyi/slot/konkr/SlotUi.kt`:
- charcoal background RGB 23/22/25, plate 37/36/43, focused 68/66/77
- warm cream text 246/244/239, subdued text 174/171/180,
  outlines 91/89/96, gold accent 210/188/134
- original upstream `label.ttf` (bundled with license), matching
  uppercase menu treatment, slight tracking and compact rectangular plates
- focus indications for controller + touch, B/Back dismissal.

## Detailed application-owned surface review

| Area | Before | After / decision |
|---|---|---|
| Shelf/menu, game menu, core picker, save-state selector | Already rendered by authentic Slot/Rust UI and source font | **Preserved**, not replaced by Android |
| RetroAchievements menu entry | Redundant 'A: RA account / sign in' modal | Direct action to logged-in dashboard / first sign-in |
| RA sign-in username/password | Default Android AlertDialog, system font/input boxes and buttons | **SlotModalDialog**, genuine Slot font, matching flat plate fields/buttons, explicit password masking with system IME, controller A/B, inline errors, securely blocking closure while HTTPS exchange is in progress; password cleared from visible field and never persisted |
| RA sign-out confirmation, including pending awards | Default Android AlertDialog with separate Material button style | **SlotModalDialog**, same chrome/button style, warns about pending awards, same-account ownership, no deletion, Internet required for re-login; B/Back cancels. Existing flush/atomic signout barrier unchanged |
| RA account panel | Full screen in original label font but duplicated local color tokens | Unified with SlotUi colors/font; full screen, controller B to return, GBA preparation and badge-image percentages; counts are account scoped |
| Achievement earned / pending / synced | Native Slot-themed Android overlay with badge cache and local-pending semantics | Shared palette/font for in-game RA overlay; compact top-left, bounded local badge loading, privacy-safe; existing 4-sided inset outline preserved |
| Ordinary application messages | Custom foreground Toast with system sans-serif font | Upstream Slot font and tokens in SlotToast. Framework may still impose limits/fallback if a background/system Toast cannot be drawn |
| Apps grid, Apps header | Android sans-serif bold, other white & charcoal tokens | Original Slot font and common text/plate colors. Original Rust-exported **real Slot controller glyph bitmaps** in footer remain unchanged; individual app icons remain vendor icons as expected |
| Cart Studio Android toolbar / loading state | Android default toolbar/system text and rounded generic buttons | Original Slot font and common palette on Android-added wrapper only; action logic unchanged |
| Cart Studio HTML/JS/WASM editor | Original upstream Studio interface | **Intentionally left unchanged**, because this is the authentic upstream tool, not a custom Slot in-app Android form |
| Library loading, storage failures, errors | Plain OS TextView and application Toast messages | Shared Slot font/colors for app-owned status & Toast |
| Android Storage Access Framework file/folder chooser | Platform UI | **Remain native** — the app does not control those secure OS screens |
| Android Home-app role chooser / system settings | OS-owned UI | **Remain native** — permission-sensitive/system-owned flows |
| Keyboard / input method | Platform IME | **Remain native** to support hardware/virtual keyboards and password security |
| Android splash and own branding | Original Slot wordmark and assets | **Preserved** |
| Future uninstall / OS permission messages | Platform UI | Not styleable from Slot; OS owns text and presentation |

## Behavioral invariants

- No change to ROM scanning, RetroArch save/state paths, emulation,
  rcheevos rules, GBA offline Smart Cache, award ledger or authentication
  transport. This patch is UI-only.
- Account sign-out retains **all** account-scoped local achievements,
  game definitions and badges; the existing native flush barrier must
  succeed before the persisted signout marker and process shutdown.
- Do not expose usernames/tokens/passwords in logs, clipboard, or overlays.
  Error messages stay localized within the styled form where possible.
- No overlay permission; popups remain foreground Activity views.
- No unverified rarity percentages.
- Native Android password/keyboard handling stays real EditText/IME,
  with original Slot typography and focus chrome only.

## Pending real KONKR acceptance

Verify landscape 960x640 layout (no cut-off text), login username field and
masked password with actual Android keyboard and gamepad A; IME Done submit;
inline error on wrong password; B cancel; full-screen RA dashboard; pending
sign-out confirmation and B cancellation; separate-account re-login after
safe close; Apps navigation and tile selection, original hint footer, and
Cart Studio wrapper toolbar. Verify notifications don't block controller input.
The original Cart Studio web editor and OS file/Home dialogs need no redesign.

Keep PR #4 **DRAFT** until physical acceptance. CI tests/checks are necessary
but do not validate focus order and keyboard rendering on the KONKR.


## Follow-up review of focus and controller chrome

The shared `SlotUi.hint` renders A/B keycaps in the source Slot cream/charcoal
font palette; this is reused in the full-screen RA dashboard (B Back) and in
both sign-in and sign-out modals (A Select / B Back), instead of plain Unicode
key characters. The Apps footer retains the **pixel-exact** upstream-exported
keycap images from the prior PR.

The login form starts with focus on Username without automatically summoning
the Android keyboard. Gamepad A opens the system keyboard on an EditText, B
dismisses the dialog, and IME Done attempts login. Errors are rendered after
the password field, not in a separate Android Toast, and remain readable
within the scroll container. The OS password transformation is configured
before assigning the original Slot font; no password is written to disk.

A dedicated, app-owned dialog explicitly requests `FEATURE_NO_TITLE` before
inflating content, so there is no accidental Android/Material title bar
behind Slot's own heading. Only the actual Android IME remains system-styled.
