#!/usr/bin/env python3
"""Package pinned original Cart Studio with only the KONKR SAF adapter.

Must run after official wasm-pack compilation. No emulation/runtime files,
original renderer, fonts, sprites, or Studio UI logic are rewritten.
"""
from pathlib import Path
import argparse
import shutil

def checked_replace(text, old, new):
    if text.count(old) != 1:
        raise RuntimeError("Official Studio changed; review Android adapter: " + old[:90])
    return text.replace(old, new)

def package(root: Path, built: Path):
    source = root / "third_party/slot-cart-studio/web"
    target = root / "android/app/src/main/assets/studio"
    assert (source / "studio.js").is_file()
    assert (built / "pkg/slot_cart_studio_bg.wasm").is_file()
    target.mkdir(parents=True, exist_ok=True)

    for file in source.glob("*"):
        if file.is_file() and file.suffix in {".js", ".css", ".html"}:
            shutil.copy2(file, target / file.name)
    (target / "pkg").mkdir(exist_ok=True)
    for name in ("slot_cart_studio.js", "slot_cart_studio_bg.wasm"):
        shutil.copy2(built / "pkg" / name, target / "pkg" / name)
    shutil.copy2(root / "android/studio/android-card.js", target / "android-card.js")
    # Only the Android-packaged web app gets compact embedded CSS.
    shutil.copy2(root / "android/studio/embedded.css", target / "embedded.css")

    jsfile = target / "studio.js"
    js = jsfile.read_text("utf-8")
    js = "import { fromAndroid } from './android-card.js';\n" + js
    js = checked_replace(js, "  const crc = new Crc32();",
                         "  if (file.slotCrc != null) return file.slotCrc >>> 0;\n  const crc = new Crc32();")
    # Selected cart (X) enters its editor immediately, while the normal menu
    # displays the official three-tab catalog with Real Label/Logo Only prompt.
    # Selected-first order also makes ROM matching responsive in a large library.
    js = checked_replace(js,
                         "  showPlatform(platformsOf(session)[0] ?? PLATFORMS[0]);",
                         """  const selected = source.selectedKey &&
      session.carts.find((c) => c.slotId === source.selectedKey);
  showPlatform(selected?.platform ?? platformsOf(session)[0] ?? PLATFORMS[0]);
  if (window.AndroidStudio && selected) editor.open(selected);""")
    js = checked_replace(js, "  document.body.dataset.ready = 'true';" ,
        """  document.body.dataset.ready = 'true';
  if (window.AndroidStudio) {
    $('pick').hidden = true;
    $('pick-files-label').hidden = true;
    try {
      await open(fromAndroid(window.AndroidStudio));
    } catch (error) {
      console.error('KONKR Cart Studio:', error);
      banner('Could not open selected cartridge: ' + error.message);
    }
  }""")
    # Load the official art index concurrently with local library setup:
    # the three tabs and edited cartridge remain usable with slow/offline Wi-Fi.
    js = checked_replace(js, "let artIndex = null;",
        "let artIndex = null;\\nlet androidIndexReady = Promise.resolve();")
    js = checked_replace(js,
        "async function match(s) {\\n  // One database per platform",
        "async function match(s) {\\n  await androidIndexReady;\\n  // One database per platform")
    js = checked_replace(js,
        """  if (ART_BASE) {
    try {
      artIndex = await fetchIndex(ART_BASE);
      console.info(`art set: ${Object.keys(artIndex).length} checksums from ${ART_BASE}`);
    } catch (e) {
      banner(`The art set at ${ART_BASE} didn’t load (${e.message}), so carts get slot’s own labels.`);
    }
  }""",
        """  if (ART_BASE) {
    const loadIndex = async () => {
      try {
        artIndex = await fetchIndex(ART_BASE);
        console.info(`art set: ${Object.keys(artIndex).length} checksums from ${ART_BASE}`);
      } catch (e) {
        banner(`The art set at ${ART_BASE} didn’t load (${e.message}), so carts get slot’s own labels.`);
      }
    };
    if (window.AndroidStudio) {
      // Never block entering the editor on a remote service.
      androidIndexReady = Promise.race([
        loadIndex(),
        new Promise((resolve) => setTimeout(resolve, 12000)),
      ]);
    } else {
      await loadIndex();
    }
  }""")
    js = js.replace("'Write to card'", "'Save to Slot'")
    jsfile.write_text(js, encoding="utf-8")

    htmlfile = target / "index.html"
    html = htmlfile.read_text("utf-8")
    html = checked_replace(html, '<meta charset="utf-8">',
        """<meta charset="utf-8">
<meta http-equiv="Content-Security-Policy" content="
 default-src 'self';
 script-src 'self' 'wasm-unsafe-eval';
 style-src 'self' 'unsafe-inline' https://fonts.googleapis.com;
 font-src 'self' https://fonts.gstatic.com data:;
 connect-src 'self' https://raw.githubusercontent.com https://art.slot-cfw.fyi;
 img-src 'self' data: blob: https://raw.githubusercontent.com https://art.slot-cfw.fyi;
 object-src 'none'; frame-src 'none'; base-uri 'self'; form-action 'none';
">""")
    html = checked_replace(html, '<link rel="stylesheet" href="studio.css">',
        '<link rel="stylesheet" href="studio.css">\n<link rel="stylesheet" href="embedded.css">')
    htmlfile.write_text(html, encoding="utf-8")
    assert 'editor.open(selected)' in js
    assert 'embedded.css' in html
    assert (target / 'embedded.css').is_file()
    print("Original Cart Studio packaged:", target,
          (target / "pkg/slot_cart_studio_bg.wasm").stat().st_size, "WASM bytes")

if __name__ == "__main__":
    ap = argparse.ArgumentParser()
    ap.add_argument("--source", default=".")
    ap.add_argument("--built", required=True)
    args = ap.parse_args()
    package(Path(args.source), Path(args.built))
