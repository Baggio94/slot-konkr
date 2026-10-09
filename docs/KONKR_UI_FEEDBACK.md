# KONKR Pocket Advance — Slot UX and input parity

Sources of truth:
- Original Slot controls: https://github.com/BrandonKowalski/slot/blob/main/site/index.html
- Original slot Cart Studio: https://github.com/BrandonKowalski/slot-cart-studio
- Portable game is the user-approved `/ROMs` SAF directory; never modify/relocate it.

## Immediate confirmed feedback (M4 polish)

1. Insert animation (about 730ms) runs to completion before opening the mGBA
   core and restoring its previous state. ROM pre-staging may run in parallel;
   display one full seated frame before transitioning. Cancelled/late ROM
   preparations must not start the wrong cart.
2. L1/R1 skips empty consoles. With only GBA titles, neither switches to
   blank GB/GBC shelves. A newly populated console appears upon library rescan.

## KONKR-specific controller mapping (planned, not yet implemented)

A logical **HOME/MENU** button should map to the physical customizable round
button beside L2, IF Android delivers a remappable key event. This is Slot's
MENU command, distinct from Android's reserved system HOME/launcher action.
Before implementation, inspect the physical event with `adb shell getevent -lt`
while tapping only that top button; never assume KEYCODE_HOME can be captured.

### Carousel

| Control | UX |
| --- | --- |
| Left/Right | previous/next cart |
| Up/Down | previous/next letter |
| L1/R1 | previous/next populated platform only |
| Tap A | complete cart insertion, then resume previous save state |
| Hold A | complete cart insertion, then fresh boot without state restore |
| B | cancel pending insertion |
| START | compact library popup: choose ROM folder, refresh, scrape missing labels; optionally Settings |
| SELECT | flip/open cartridge and choose installed core (mGBA initially, gpSP when available) |
| physical HOME/MENU | Slot settings/quick menu (NOT Android's system HOME) |

The last two rows intentionally differ from upstream: original Slot uses START
for the core picker. KONKR reserves START for a library management menu, so
SELECT gets the core picker. Upstream MENU remains HOME/MENU.

### In game

| Input | Original Slot behavior to port |
| --- | --- |
| Hold HOME/MENU | autosave, eject cart, return to carousel |
| Double tap HOME/MENU | save-state browser |
| SELECT + R1 | create save state |
| SELECT + L1 | load recent save state |
| SELECT + B / A | previous/next shader |
| SELECT + Y | toggle color correction |
| SELECT + L2/R2 | GB palette previous/next |
| Hold L2 | rewind |
| Hold R2 | fast forward |
| Double tap R2 | toggle permanent fast forward |
| Hold X/Y | turbo A/turbo B |
| L1/R1 | stretch / original ratio (GB/GBC only) |
| SELECT + HOME/MENU | link-play screen (requires later networking support) |
| SELECT + Up/Down | screen brightness (Android per-window control where possible) |
| SELECT + Left/Right | blue light / color temperature |
| START and SELECT individually | forwarded to core for gameplay |

The old KONKR prototype START+SELECT exit shortcut must be removed once a
working HOME/MENU button is available. Never route a host hotkey to mGBA
input simultaneously; save/load combos must not trigger in-game buttons.
Features missing in current 0.0.3 (rewind, shaders, switcher, gpSP, link)
remain TODO; simply specifying a key combination does not implement them.

### ROM art / labels (planned)

Use the look and format of **Slot Cart Studio**, not generic fullscreen boxart.
Source: `slot-cart-studio/src/label.rs` produces stylized logos over gradient
backgrounds, and offers actual printed label artwork. Studio matches games
using CRC32/No-Intro and ScreenScraper logos, allows editing color/shell, and
exports PNGs that original Slot can use as cartridge labels.

Proposed priority for each ROM:
1. user-custom label imported from Cart Studio (never overwrite),
2. cached automatic Studio-style/generated label,
3. existing generated text-only label when artwork is missing or offline.

An optional “Scrape missing labels” workflow should cache artwork and match by
hash/platform, show failed/ambiguous matches, and permit per-cartridge replace.
Do not scrape on the renderer or slow down shelf scrolling. Credit upstream and
review provenance/licenses for ScreenScraper media and libretro-database data.
Optionally reuse existing ROMM metadata/artwork where available, but Android
must not require a Raspberry Pi or external library server.

## Test checklist

- GBA-only shelf remains unchanged on L1 and R1.
- GBA+GBC and GBA+GB skip empty console in either direction.
- Empty library has no crashes and no phantom inserted carts.
- A normal and a previously played game both display 100% insertion
  before mGBA initializes and save state resumes.
- B cancels even while asynchronous Android SAF staging is ongoing; no
  stale GameReady/GameError launches or dismisses a new cart.
- Match button round beside L2 to a valid nonreserved key event.
- Once input router is implemented: long/short/double presses and chords do
  not leak gameplay input, no accidental app exit or Android launcher switch.
