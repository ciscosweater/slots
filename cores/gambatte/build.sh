#!/bin/sh
set -eu

commit="$1"
work="$2"
out="$3"
archive="${out}.src.tar.gz"
mkdir -p "$work" "$(dirname "$out")"

if [ ! -f "$archive" ]; then
    curl -fsSL -o "$archive" "https://github.com/libretro/gambatte-libretro/archive/${commit}.tar.gz"
fi
rm -rf "$work/src"
mkdir -p "$work/src"
tar -xzf "$archive" -C "$work/src" --strip-components=1

case "$(uname -s)" in
    Darwin) platform=osx; built=gambatte_libretro.dylib ;;
    Linux) platform=unix; built=gambatte_libretro.so ;;
    *) echo "unsupported host $(uname -s)" >&2; exit 1 ;;
esac

make -C "$work/src" -f Makefile.libretro platform="$platform" -j"$(getconf _NPROCESSORS_ONLN)"
cp "$work/src/$built" "$out"
printf 'commit=%s\nsource=https://github.com/libretro/gambatte-libretro/tree/%s\n' "$commit" "$commit" > "${out}.meta"
