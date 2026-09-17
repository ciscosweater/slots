#!/usr/bin/env bash
set -Eeuo pipefail
IFS=$'\n\t'

script_dir="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
repo_root="$(cd -- "$script_dir/.." && pwd)"

usage() {
	cat <<'USAGE'
Usage: scripts/build-release.sh [--out DIR]

Build the aarch64 device tree used by AGS-102 and BaseOS.

DIR is relative to the repository root unless it is absolute inside the
repository. The default is dist-device. The build is delegated to the
dist:device task, which also builds the device binary and all three libretro
cores. On aarch64 Linux the task builds natively; everywhere else it enters
the LoveRetro H700 toolchain container (see scripts/with-h700-toolchain.sh).
Keeping DIR inside the repository is required when Task uses Docker, because
only the repository is mounted into the build container.
USAGE
}

die() {
	echo "build-release.sh: $*" >&2
	exit 1
}

out="$repo_root/dist-device"

while (($#)); do
	case "$1" in
		--out|--output)
			(($# >= 2)) || die "$1 requires a directory"
			out="$2"
			shift 2
			;;
		--help|-h)
			usage
			exit 0
			;;
		*)
			die "unknown argument '$1' (use --help for usage)"
			;;
	esac
done

if [[ "$out" == /* ]]; then
	case "$out" in
		"$repo_root"/*)
			task_out="${out#"$repo_root"/}"
			;;
		*)
			die "output directory must be inside the repository: $out"
			;;
	esac
else
	task_out="$out"
	out="$repo_root/$task_out"
fi

case "$task_out" in
	""|.|./|..|../*|*/../*|*/..)
		die "refusing to use a repository or filesystem root as the output directory"
		;;
esac

[[ "$task_out" != *[[:space:]]* ]] ||
	die "output directory cannot contain whitespace because Task interpolates it"

task_bin=""
for candidate in task go-task; do
	if command -v "$candidate" >/dev/null 2>&1; then
		task_bin="$candidate"
		break
	fi
done
[[ -n "$task_bin" ]] ||
	die "Task is required; on CachyOS/Arch: pacman -S go-task (binary may be go-task)"

echo "Building device tree: $out"
(
	cd -- "$repo_root"
	"$task_bin" dist:device "OUT=$task_out"
)
[[ -f "$out/System/slot" ]] ||
	die "Task completed without producing $out/System/slot"
echo "Device tree ready: $out"
