#!/usr/bin/env bash
set -Eeuo pipefail
IFS=$'\n\t'

usage() {
	cat <<'USAGE'
Usage: scripts/verify-release.sh TREE_OR_ZIP

Verify a device tree or release ZIP before it is copied to an SD card.
Checks the directory layout, executable bits, aarch64 binaries, BaseOS
launcher, license files, and the corresponding source archives/metadata.
USAGE
}

die() {
	echo "verify-release.sh: $*" >&2
	exit 1
}

[[ $# -eq 1 ]] || {
	usage >&2
	exit 2
}

input="$1"
[[ "$input" != "--help" && "$input" != "-h" ]] || {
	usage
	exit 0
}

for command_name in file grep tar unzip; do
	command -v "$command_name" >/dev/null 2>&1 || die "'$command_name' is required"
done

if command -v sha256sum >/dev/null 2>&1; then
	sha256_file() {
		sha256sum "$1" | awk '{ print $1 }'
	}
elif command -v shasum >/dev/null 2>&1; then
	sha256_file() {
		shasum -a 256 "$1" | awk '{ print $1 }'
	}
else
	die "sha256sum or shasum is required"
fi

tmp="$(mktemp -d "${TMPDIR:-/tmp}/slot-verify.XXXXXX")"
cleanup() {
	rm -rf -- "$tmp"
}
trap cleanup EXIT

base="$input"
if [[ -f "$input" ]]; then
	[[ "$input" == *.zip ]] || die "file is not a ZIP archive: $input"
	unzip -tq "$input" || die "ZIP archive is corrupt: $input"
	base="$tmp/unpacked"
	mkdir -p -- "$base"
	unzip -q "$input" -d "$base"
elif [[ ! -d "$input" ]]; then
	die "path does not exist: $input"
fi

root=""
if [[ -f "$base/System/slot" ]]; then
	root="$base"
else
	mapfile -t children < <(find "$base" -mindepth 1 -maxdepth 1 -type d -print)
	if ((${#children[@]} == 1)) && [[ -f "${children[0]}/System/slot" ]]; then
		root="${children[0]}"
	fi
fi
[[ -n "$root" ]] || die "could not find a release root containing System/slot"

required_directories=(
	BIOS
	Games
	Labels
	Saves
	States
	System
	System/licenses
	Wallpapers
	.system/h700/paks/MinUI.pak
)
for relative_path in "${required_directories[@]}"; do
	[[ -d "$root/$relative_path" ]] || die "missing directory: $relative_path"
done

required_files=(
	System/slot
	System/mgba_libretro.so
	System/gpsp_libretro.so
	System/gambatte_libretro.so
	.system/h700/paks/MinUI.pak/launch.sh
	System/licenses/README.md
	System/licenses/mgba-MPL-2.0.txt
	System/licenses/gpsp-GPL-2.0.txt
	System/licenses/gambatte-GPL-2.0.txt
)
for relative_path in "${required_files[@]}"; do
	[[ -f "$root/$relative_path" ]] || die "missing file: $relative_path"
done

[[ -x "$root/System/slot" ]] || die "System/slot is not executable"
[[ -x "$root/.system/h700/paks/MinUI.pak/launch.sh" ]] || die "BaseOS launch.sh is not executable"
grep -q 'SLOT_ROOT=' "$root/.system/h700/paks/MinUI.pak/launch.sh" ||
	die "BaseOS launch.sh does not set SLOT_ROOT"

require_aarch64() {
	local relative_path="$1"
	local description
	description="$(file -b "$root/$relative_path")"
	if ! grep -Eiq 'aarch64|ARM64' <<<"$description"; then
		die "$relative_path is not an aarch64 binary: $description"
	fi
}

require_aarch64 System/slot
require_aarch64 System/mgba_libretro.so
require_aarch64 System/gpsp_libretro.so
require_aarch64 System/gambatte_libretro.so

shopt -s nullglob
licenses="$root/System/licenses"

read_commit() {
	local metadata="$1"
	awk -F= '$1 == "commit" { print $2; exit }' "$metadata"
}

check_source_pair() {
	local prefix="$1"
	local metadata_files=( "$licenses"/"$prefix"-*.meta )
	local metadata filename named_commit recorded_commit archive

	((${#metadata_files[@]} == 1)) ||
		die "expected exactly one $prefix metadata file, found ${#metadata_files[@]}"

	metadata="${metadata_files[0]}"
	filename="${metadata##*/}"
	named_commit="${filename#"$prefix-"}"
	named_commit="${named_commit%.meta}"
	recorded_commit="$(read_commit "$metadata")"

	[[ -n "$named_commit" && "$named_commit" == "$recorded_commit" ]] ||
		die "$filename does not name the commit recorded inside it"

	archive="$licenses/$prefix-$named_commit.tar.gz"
	[[ -f "$archive" ]] || die "missing corresponding source archive: ${archive##*/}"
	tar -tzf "$archive" >/dev/null || die "invalid source archive: ${archive##*/}"
}

check_source_pair gambatte
check_source_pair gpsp

mgba_metadata=( "$licenses"/mgba-*.meta )
((${#mgba_metadata[@]} == 1)) ||
	die "expected exactly one mGBA metadata file, found ${#mgba_metadata[@]}"

mgba_patch_records=()
while IFS= read -r patch_record; do
	[[ -n "$patch_record" ]] && mgba_patch_records+=("$patch_record")
done < <(grep '^patch=' "${mgba_metadata[0]}" || true)
((${#mgba_patch_records[@]} >= 1)) || die "mGBA metadata does not list any patch"

for patch_record in "${mgba_patch_records[@]}"; do
	patch_spec="${patch_record#patch=}"
	patch_name="${patch_spec%% sha256:*}"
	expected_hash="${patch_spec#*sha256:}"
	[[ "$patch_spec" == *" sha256:"* && -n "$patch_name" && -n "$expected_hash" ]] ||
		die "malformed mGBA patch metadata: $patch_record"
	[[ "$(basename "$patch_name")" == "$patch_name" ]] ||
		die "unsafe mGBA patch name in metadata: $patch_name"

	shipped_patch="$licenses/mgba-$patch_name"
	[[ -f "$shipped_patch" ]] ||
		die "missing shipped mGBA patch: ${shipped_patch##*/}"
	actual_hash="$(sha256_file "$shipped_patch")"
	[[ "$actual_hash" == "$expected_hash" ]] ||
		die "mGBA patch checksum mismatch: ${shipped_patch##*/}"
done

mgba_filename="${mgba_metadata[0]##*/}"
mgba_named_commit="${mgba_filename#mgba-}"
mgba_named_commit="${mgba_named_commit%.meta}"
mgba_recorded_commit="$(read_commit "${mgba_metadata[0]}")"
[[ -n "$mgba_named_commit" && "$mgba_named_commit" == "$mgba_recorded_commit" ]] ||
	die "$mgba_filename does not name the commit recorded inside it"

echo "Verified release: $root"
echo "  device binary: $(file -b "$root/System/slot")"
echo "  cores: mGBA, gpSP, Gambatte (aarch64)"
echo "  sources: Gambatte, gpSP, and mGBA patch metadata present"
