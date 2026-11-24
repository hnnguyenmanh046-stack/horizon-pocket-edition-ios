#!/usr/bin/env python3
"""Android-only statements taken out of the game files the mods hook, in a build's copy.

    native_stage_cfg.py SOURCE MODS

Run by native_stage.sh on its copy of the source (never the checkout), after native_mod_adapt.py
has pointed the mods' targets at where things are. horizon-mixins refuses to hook a function whose
body has a conditional attribute it cannot resolve (E_UNSUPPORTED_CFG): since Starling's wardrobe
(2026-11-18) the Options screen's `draw_options` has one,

    #[cfg(target_os = "android")]
    self.draw_wardrobe_preview(batch);

and Better Dynamic Lights hooks it. What native_stage.sh leaves is built for iOS (ios.yml) or
checked for the desktop (native-mods-check.yml), never for Android, so such a statement is not
compiled there either way: with its attribute it goes from the copy, and the function is plain
again. Only whole one-line statements inside a body, only in files a mod of MODS targets; anything
else stays, and horizon-mixins names it if a mod needs it.
"""

import pathlib
import re
import sys

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent.parent / "android"))
import native_mod_adapt as adapt  # noqa: E402

ANDROID = '#[cfg(target_os = "android")]'
TARGET = re.compile(r'^\s*target\s*=\s*"([^"]*)"', re.M)


def statement(line: str) -> bool:
    """Whether `line` is one whole statement inside a function body."""
    text = line.strip()
    if not line.startswith(" " * 8) or not text.endswith(";") or text.startswith(("#", "//")):
        return False
    return all(text.count(a) == text.count(b) for a, b in ("()", "[]", "{}"))


def without_android(text: str) -> tuple[str, int]:
    lines = text.split("\n")
    out, gone, i = [], 0, 0
    while i < len(lines):
        if lines[i].strip() == ANDROID and i + 1 < len(lines) and statement(lines[i + 1]):
            gone += 1
            i += 2
            continue
        out.append(lines[i])
        i += 1
    return "\n".join(out), gone


def targeted_files(source: pathlib.Path, mods: pathlib.Path) -> list[str]:
    tree = adapt.module_tree(source)
    files = set()
    for manifest in sorted(mods.glob("*/horizon-mixin.toml")):
        for target in TARGET.findall(manifest.read_text(encoding="utf-8")):
            split = adapt.split_target(target)
            if split and split[0] in tree and tree[split[0]].file:
                files.add(tree[split[0]].file)
    return sorted(files)


def self_test() -> None:
    text = "\n".join([
        "impl Game<'_> {",
        "    fn draw(&mut self) {",
        "        self.a();",
        '        #[cfg(target_os = "android")]',
        "        self.wardrobe(batch);",
        '        #[cfg(target_os = "android")]',
        "        if self.b() {",
        "            self.c();",
        "        }",
        '        #[cfg(not(target_os = "android"))]',
        "        self.d();",
        "    }",
        '    #[cfg(target_os = "android")]',
        "    fn only(&self);",
        "}",
    ])
    new, gone = without_android(text)
    assert gone == 1, gone
    assert "wardrobe" not in new and "self.a();\n        #[cfg" in new
    # A block, another condition and an item keep their attribute.
    assert new.count(ANDROID) == 2 and "self.d();" in new and "fn only" in new
    assert without_android(new) == (new, 0)
    print("native_stage_cfg: ok")


def main() -> None:
    if sys.argv[1:] == ["--self-test"]:
        return self_test()
    if len(sys.argv) != 3:
        sys.exit(__doc__)
    source, mods = pathlib.Path(sys.argv[1]), pathlib.Path(sys.argv[2])
    for rel in targeted_files(source, mods):
        path = source / rel
        new, gone = without_android(path.read_text(encoding="utf-8"))
        if gone:
            path.write_text(new, encoding="utf-8")
            print(f"native_stage_cfg: {rel}: {gone} Android-only statement(s) left out of this build's copy")


if __name__ == "__main__":
    main()
