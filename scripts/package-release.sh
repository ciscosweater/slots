#!/usr/bin/env bash
set -Eeuo pipefail
IFS=$'\n\t'

script_dir="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
repo_root="$(cd -- "$script_dir/.." && pwd)"

usage() {
	cat <<'USAGE'
Usage: scripts/package-release.sh VERSION [options]

Build, verify, and package a H700 release. VERSION may be written as 1.0.0
or v1.0.0. The resulting files are written to dist/releases by default:

  slots-vVERSION-h700.zip
  slots-vVERSION-h700.zip.sha256

Options:
  --tree DIR          Package an existing device tree instead of building it.
  --output-dir DIR    Directory for the ZIP and checksum (default: dist/releases).
  --help              Show this help.
USAGE
}

die() {
	echo "package-release.sh: $*" >&2
	exit 1
}

version=""
tree=""
output_dir="$repo_root/dist/releases"

while (($#)); do
	case "$1" in
		--tree)
			(($# >= 2)) || die "--tree requires a directory"
			tree="$2"
			shift 2
			;;
		--output-dir|--out)
			(($# >= 2)) || die "$1 requires a directory"
			output_dir="$2"
			shift 2
			;;
		--help|-h)
			usage
			exit 0
			;;
		-*)
			die "unknown option '$1' (use --help for usage)"
			;;
		*)
			[[ -z "$version" ]] || die "VERSION was provided more than once"
			version="$1"
			shift
			;;
	esac
done

[[ -n "$version" ]] || {
	usage >&2
	exit 2
}

if [[ "$version" == v* || "$version" != [0-9]* ]]; then
	tag="$version"
else
	tag="v$version"
fi

[[ "$tag" != "v" ]] || die "VERSION cannot be empty"
[[ "$tag" != *[![:alnum:]._-]* ]] ||
	die "VERSION may contain only letters, numbers, dots, underscores, and hyphens"

command -v zip >/dev/null 2>&1 || die "'zip' is required"
if ! command -v sha256sum >/dev/null 2>&1 && ! command -v shasum >/dev/null 2>&1; then
	die "sha256sum or shasum is required"
fi

if [[ "$tree" == /* ]]; then
	:
elif [[ -n "$tree" ]]; then
	tree="$PWD/$tree"
fi

if [[ "$output_dir" != /* ]]; then
	output_dir="$PWD/$output_dir"
fi
mkdir -p -- "$output_dir"
output_dir="$(cd -- "$output_dir" && pwd)"

mkdir -p -- "$repo_root/dist"
# Keep the temporary build tree under the repository: with-h700-toolchain.sh mounts the
# repository as /src, so an external absolute path would be invisible inside that container.
# /dist is already gitignored.
tmp="$(mktemp -d "$repo_root/dist/.slot-release.XXXXXX")"
cleanup() {
	rm -rf -- "$tmp"
}
trap cleanup EXIT

if [[ -z "$tree" ]]; then
	tree="$tmp/tree"
	"$script_dir/build-release.sh" --out "$tree"
else
	[[ -d "$tree" ]] || die "release tree does not exist: $tree"
fi

"$script_dir/verify-release.sh" "$tree"

package_name="slots-$tag"
archive_name="$package_name-h700.zip"
checksum_name="$archive_name.sha256"
package_root="$tmp/$package_name"
mkdir -p -- "$package_root"

# Copy through `/.` so hidden paths, especially .system, are included without
# making the supplied tree itself part of the archive under a different name.
cp -a -- "$tree"/. "$package_root"/

archive_tmp="$tmp/$archive_name"
(
	cd -- "$tmp"
	zip -qrX "$archive_tmp" "$package_name"
)

mv -f -- "$archive_tmp" "$output_dir/$archive_name"
"$script_dir/verify-release.sh" "$output_dir/$archive_name"

checksum_tmp="$tmp/$checksum_name"
if command -v sha256sum >/dev/null 2>&1; then
	(
		cd -- "$output_dir"
		sha256sum "$archive_name"
	) >"$checksum_tmp"
elif command -v shasum >/dev/null 2>&1; then
	(
		cd -- "$output_dir"
		shasum -a 256 "$archive_name"
	) >"$checksum_tmp"
else
	die "sha256sum or shasum is required"
fi
mv -f -- "$checksum_tmp" "$output_dir/$checksum_name"

echo "Release ZIP: $output_dir/$archive_name"
echo "SHA-256:     $output_dir/$checksum_name"
