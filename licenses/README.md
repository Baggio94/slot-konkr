# Licenses

`slot` is **GPL-3.0-or-later**. Copyright (C) 2026 Brandon T. Kowalski.

| Core                | Source                                             | License              | Text here                  |
|---------------------|----------------------------------------------------|----------------------|----------------------------|
| `gpsp_libretro`     | https://github.com/libretro/gpsp                   | GPL-2.0-or-later     | `gpsp-GPL-2.0.txt`         |
| `mgba_libretro`     | https://github.com/libretro/mgba                   | MPL-2.0              | `mgba-MPL-2.0.txt`         |

- **MPL-2.0 (mGBA): this build is modified, and the modifications ship in this directory.**
  The core is libretro/mgba at the commit recorded in `mgba-<commit>.meta`, with every patch
  from `cores/mgba/` applied.

  The one patch today is upstream mGBA's own fix for the Classic NES Series audio,
  https://github.com/mgba-emu/mgba/commit/685023e05d90d87050fb357f46f7bd2d907083f5, which
  libretro/mgba had not picked up when this build was set up.
