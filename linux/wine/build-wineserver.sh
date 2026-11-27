#!/usr/bin/env bash
# Builds GE-Proton's own wineserver with this fork's patches (linux/wine/*.patch)
# and installs it into a copy of that GE-Proton: <proton>-victory, next to it.
# The original GE-Proton is not changed. Point victory-sandbox at the copy:
#
#   victory-sandbox setup --proton ~/.local/share/Steam/compatibilitytools.d/GE-Proton11-14-x86_64-victory
#
# Only the server is built, from GE's exact Wine tree (the ValveSoftware wine
# and wine-staging commits its tag pins, with GE's patch script applied), so
# the server protocol matches the GE-Proton it goes into.
#
# Usage: build-wineserver.sh [GE tag] [GE-Proton dir] [work dir]
set -euo pipefail

tag=${1:-GE-Proton11-14}
proton=${2:-$HOME/.local/share/Steam/compatibilitytools.d/${tag}-x86_64}
work=${3:-$HOME/Dokumente/Rust/victory/upstream-src}
here=$(cd "$(dirname "$0")" && pwd)

[ -x "$proton/files/bin/wineserver" ] || { echo "no GE-Proton at $proton"; exit 1; }
mkdir -p "$work"
cd "$work"

# 1. GE's sources at the tag, with the Wine submodules it pins.
if [ ! -d ge-proton/.git ]; then
    git clone --depth 1 --branch "$tag" https://github.com/GloriousEggroll/proton-ge-custom.git ge-proton
fi
cd ge-proton
git submodule update --init --depth 1 wine wine-staging wineopenxr

# 2. GE's Wine patching (sections 2-x of its prep script), from a clean tree.
(cd wine && git reset -q --hard HEAD && git clean -qxdf)
script=patches/protonprep-valve-staging.sh
start=$(grep -n '^### (2) WINE PATCHING ###' "$script" | cut -d: -f1)
end=$(grep -n '^### END WINE PATCHING ###' "$script" | cut -d: -f1)
{ sed -n '1,15p' "$script"; sed -n "${start},${end}p" "$script"; } > ../ge-wine-prep.sh
echo "applying GE's Wine patches (log: $work/ge-wine-prep.log)"
bash ../ge-wine-prep.sh > ../ge-wine-prep.log 2>&1

# 3. This fork's patches.
cd wine
for p in "$here"/*.patch; do
    echo "applying $(basename "$p")"
    patch -Np1 < "$p"
done
# Generated headers configure needs (as GE's Makefile does after the source
# step; vk.xml and video.xml are in the tree, nothing is downloaded).
XDG_CACHE_HOME="$work/build-cache" dlls/winevulkan/make_vulkan -x vk.xml -X video.xml > ../make_vulkan.log 2>&1 \
    || { tail -20 ../make_vulkan.log; exit 1; }
tools/make_specfiles > ../make_specfiles.log 2>&1 || { tail -20 ../make_specfiles.log; exit 1; }

# 4. Only the server (64-bit host build, no PE side).
mkdir -p ../build-server
cd ../build-server
rm -rf ./*
if [ ! -f Makefile ]; then
    ../wine/configure --enable-win64 --disable-tests --without-x --without-wayland \
        --without-freetype --without-gstreamer --without-vulkan --without-mingw > configure.log 2>&1 \
        || { tail -20 configure.log; exit 1; }
fi
make -j"$(nproc)" server/wineserver > make.log 2>&1 || { tail -30 make.log; exit 1; }

# 5. A copy of GE-Proton with this server (hard links: no second copy of
#    the files; the server is replaced, not written through its link).
copy="${proton}-victory"
if [ ! -d "$copy" ]; then
    cp -al "$proton" "$copy"
    # Its own name in Steam (sed writes a new file: the link to the original goes).
    name=$(basename "$proton")
    sed -i "s/\"${name}\"/\"${name}-victory\"/; s/\"display_name\" *\"\([^\"]*\)\"/\"display_name\" \"\1 (VIctory)\"/" "$copy/compatibilitytool.vdf"
fi
rm -f "$copy/files/bin/wineserver"
cp server/wineserver "$copy/files/bin/wineserver"
echo "installed: $copy/files/bin/wineserver"
echo "use it:    victory-sandbox setup --proton $copy"
