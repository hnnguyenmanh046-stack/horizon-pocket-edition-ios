#!/usr/bin/env bash
# Starling for iOS with native mods (docs/ios.md): the mods' horizon-mixins bundles applied to
# Starling's source, as packaging/android/native_build.sh applies them for Android's builds. iOS
# runs no code its app did not ship with, so where Android swaps the game's library, the whole app
# is built from the source this leaves (.github/workflows/ios.yml).
#
#   native_stage.sh SOURCE BUNDLES
#
# SOURCE is a checkout at the commit to build: its HEAD is read (git archive), the mods are applied
# to that, and the result is written over SOURCE's working tree (its target/ and .git/ untouched),
# which is then built as usual. BUNDLES holds one folder a mod, each with a horizon-mixin.toml
# (tools/ios_native_mods.py fetch). Runs on macOS and Linux.
#
# NATIVE_CACHE (default ~/.cache/starling-native) keeps horizon-mixins between builds.
set -euo pipefail

here=$(cd "$(dirname "$0")" && pwd)
MIXINS_GIT=https://github.com/horizon-rust/horizon-mixins
# Starling's (packaging/android/native_build.sh MIXINS_REV), which is Horizon's launcher's.
MIXINS_REV=$(sed -n 's/^MIXINS_REV=\([0-9a-f]\{40\}\)$/\1/p' "$here/../android/native_build.sh")

fail() { echo "native_stage: $*" >&2; exit 1; }
[ $# -eq 2 ] || fail "usage: native_stage.sh SOURCE BUNDLES"
[ -n "$MIXINS_REV" ] || fail "packaging/android/native_build.sh no longer says MIXINS_REV"
# Real paths: horizon-mixins refuses output paths through a symlink (macOS's /var is one).
source=$(cd "$1" && pwd -P)
bundles=$(cd "$2" && pwd -P)
cache=${NATIVE_CACHE:-$HOME/.cache/starling-native}
mkdir -p "$cache"
cache=$(cd "$cache" && pwd -P)

tools=$cache/mixins-${MIXINS_REV:0:12}
if [ ! -x "$tools/bin/horizon-mixins" ]; then
  cargo install --locked --git "$MIXINS_GIT" --rev "$MIXINS_REV" --root "$tools" horizon-mixins-cli
fi

commit=$(git -C "$source" rev-parse HEAD)
# horizon-mod-host's build script names the game build by `git rev-parse HEAD` unless this says
# it (as native_build.sh): the same for the build that follows, which ios.yml exports too.
export HORIZON_GAME_BUILD_ID=$commit
work=$(cd "$(mktemp -d)" && pwd -P)
trap 'rm -rf "$work"' EXIT

# The source as committed, without links (horizon-mixins refuses them and nothing needs them).
mkdir "$work/source"
git -C "$source" archive --format=tar HEAD | tar -x -C "$work/source"
find "$work/source" -type l -delete
echo "$commit" > "$work/source/.horizon-revision"

# One identity for the game module, as native_build.sh: while horizon-mixins plans,
# horizon-android's library (the game module under a second name) is an empty file, and lib.rs
# loses the conditional attribute that keeps horizon-mixins from looking under it. On iOS that
# attribute also forbids unsafe code in the game crate, which a mod's own code may use.
python3 - "$work/source" <<'EOF'
import pathlib, sys
root = pathlib.Path(sys.argv[1])
manifest = root / "crates/horizon-android/Cargo.toml"
line = 'path = "../horizon-client/src/android_lib.rs"'
text = manifest.read_text(encoding="utf-8")
if text.count(line + "\n") != 1:
    sys.exit(f"native_stage: {manifest} no longer has: {line}")
manifest.write_text(text.replace(line + "\n", 'path = "plan-stub.rs"\n'), encoding="utf-8")
(root / "crates/horizon-android/plan-stub.rs").write_text("", encoding="utf-8")
lib = root / "crates/horizon-client/src/lib.rs"
attr = '#![cfg_attr(not(target_os = "android"), forbid(unsafe_code))]'
lines = lib.read_text(encoding="utf-8").split("\n")
if lines.count(attr) != 1:
    sys.exit(f"native_stage: {lib} no longer has: {attr}")
lines.remove(attr)
lib.write_text("\n".join(lines), encoding="utf-8")
EOF

# As native_build.sh, in its order. Horizon declares the game's modules in files its build script
# writes (`include!` of OUT_DIR), which horizon-mixins cannot follow: they are written out as plain
# module lines in this copy first (horizon-materialize, crates/horizon-mod-api-gen), before the
# mods are adapted: native_mod_adapt.py finds where things went in the module tree it leaves.
cargo fetch --locked --manifest-path "$work/source/Cargo.toml" > /dev/null
CARGO_TARGET_DIR=$cache/materialize-target cargo run --quiet --locked --offline \
  --manifest-path "$work/source/Cargo.toml" -p horizon-mod-api-gen --bin horizon-materialize \
  -- "$work/source"
# What horizon-mixins refuses in the fork's source, made readable in this copy: modules named
# through a parent folder move next to their module, and the prebuilt game's `#[replaceable]`
# markers go (Starling's native_mixins_source.py).
python3 "$source/packaging/android/native_mixins_source.py" "$work/source"

# The bundles, each pinned to the commit built here when it pins one (Starling's commits are not
# Horizon's; a mod that calls what Starling changed fails in the compiler, which names it).
mkdir "$work/mods"
count=0
for bundle in "$bundles"/*/; do
  bundle=${bundle%/}
  [ -f "$bundle/horizon-mixin.toml" ] || fail "$bundle has no horizon-mixin.toml"
  cp -R "$bundle" "$work/mods/$(basename "$bundle")"
  count=$((count + 1))
done
[ "$count" -gt 0 ] || fail "no mod in $bundles"
python3 - "$work/mods" "$commit" <<'EOF'
import pathlib, re, sys
for manifest in pathlib.Path(sys.argv[1]).glob("*/horizon-mixin.toml"):
    text = manifest.read_text(encoding="utf-8")
    pinned = re.sub(r'(?m)^horizon_revision = "[0-9a-f]*"', f'horizon_revision = "{sys.argv[2]}"', text)
    if pinned != text:
        manifest.write_text(pinned, encoding="utf-8")
EOF
# Mods made before Horizon split its client into crates, pointed at where things went (Starling's).
python3 "$source/packaging/android/native_mod_adapt.py" "$work/source" "$work/mods"

echo "native_stage: $commit with $(ls "$work/mods" | tr '\n' ' ')"
"$tools/bin/horizon-mixins" apply --workspace "$work/source" --mods "$work/mods" --out "$work/stage"

# horizon-android builds android_lib.rs again: the workspace is whole for Cargo.
git -C "$source" show "HEAD:crates/horizon-android/Cargo.toml" > "$work/stage/crates/horizon-android/Cargo.toml"
rm -f "$work/stage/crates/horizon-android/plan-stub.rs"
# horizon-mixins' own note of what it hooked is no part of the source (native_build.sh leaves it
# out of its build folder too).
rm -f "$work/stage/native-hooks.json"
rm -rf "$work/stage/target" "$work/stage/.git"

# The stage over the tree. What the stage no longer has goes from the tree too: Starling's
# native_mixins_source.py moves modules next to their parents, and a module left at its old place
# as well would be found twice (native_build.sh copies with rsync --delete).
git -C "$source" ls-files -z | while IFS= read -r -d '' file; do
  [ -e "$work/stage/$file" ] || rm -f "$source/$file"
done
cp -R "$work/stage/." "$source/"
echo "native_stage: the mods are in $source"
