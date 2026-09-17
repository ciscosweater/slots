#!/usr/bin/env bash
# Run a command inside the H700 cross-toolchain container with Rust wired to its
# aarch64 linker. Used by taskfile.yml's device:sh when this machine is not already
# aarch64 Linux (CI runs natively inside rust:1-bullseye on arm runners).
#
# The LoveRetro image ships aarch64-nextui-linux-gnu (glibc 2.33 sysroot). BaseOS on
# the H700 is glibc 2.35, so binaries linked here remain forward-compatible — the same
# reason the native CI image targets an older glibc than the device.
set -Eeuo pipefail
IFS=$'\n\t'

script_dir="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
repo_root="$(cd -- "$script_dir/.." && pwd)"

usage() {
	cat <<'USAGE'
Usage: scripts/with-h700-toolchain.sh [options] -- COMMAND...
       scripts/with-h700-toolchain.sh [options] -c 'shell command'

Run COMMAND inside ghcr.io/loveretro/h700-toolchain with cargo pointed at the
image's aarch64-nextui-linux-gnu linker. Relative paths are evaluated from the
repository root (mounted at /src).

Options:
  --image NAME   Toolchain image (default: $TOOLCHAIN_IMAGE or
                 ghcr.io/loveretro/h700-toolchain)
  --help         Show this help.

Environment:
  TOOLCHAIN_IMAGE     Override the default image name.
  CONTAINER_RUNTIME   docker or podman (auto-detected).
  CARGO_HOME          Mounted into the container (default: ~/.cargo).
  RUSTUP_HOME         Mounted into the container (default: ~/.rustup).
USAGE
}

die() {
	echo "with-h700-toolchain.sh: $*" >&2
	exit 1
}

image="${TOOLCHAIN_IMAGE:-ghcr.io/loveretro/h700-toolchain}"
shell_cmd=""
args=()

while (($#)); do
	case "$1" in
		--image)
			(($# >= 2)) || die "$1 requires a value"
			image="$2"
			shift 2
			;;
		--help|-h)
			usage
			exit 0
			;;
		-c)
			(($# >= 2)) || die "$1 requires a shell command"
			shell_cmd="$2"
			shift 2
			;;
		--)
			shift
			args=("$@")
			break
			;;
		-*)
			die "unknown option '$1' (use --help for usage)"
			;;
		*)
			args=("$@")
			break
			;;
	esac
done

if [[ -n "$shell_cmd" ]]; then
	((${#args[@]} == 0)) || die "pass either -c or a command, not both"
elif ((${#args[@]} == 0)); then
	usage >&2
	exit 2
fi

runtime="${CONTAINER_RUNTIME:-}"
if [[ -z "$runtime" ]]; then
	if command -v docker >/dev/null 2>&1; then
		runtime=docker
	elif command -v podman >/dev/null 2>&1; then
		runtime=podman
	else
		die "docker or podman is required to use the H700 toolchain"
	fi
fi
command -v "$runtime" >/dev/null 2>&1 || die "'$runtime' is not available"

cargo_home="${CARGO_HOME:-$HOME/.cargo}"
rustup_home="${RUSTUP_HOME:-$HOME/.rustup}"
mkdir -p -- "$cargo_home" "$rustup_home"

# Quote the caller's command for bash -lc so spaces and metacharacters survive.
if [[ -n "$shell_cmd" ]]; then
	inner="$shell_cmd"
else
	inner="$(printf '%q ' "${args[@]}")"
	inner="${inner% }"
fi

# Host rustup is reused so the pin in rust-toolchain.toml stays authoritative; only the
# linker and sysroot come from the image. CARGO_BUILD_TARGET makes every cargo invocation
# in this shell cross-compile without each task having to pass --target.
setup='
set -Eeuo pipefail
export PATH="/root/.cargo/bin:${CROSS_ROOT:-/opt/aarch64-nextui-linux-gnu}/bin:${PATH}"
export CARGO_BUILD_TARGET=aarch64-unknown-linux-gnu
export CARGO_TARGET_AARCH64_UNKNOWN_LINUX_GNU_LINKER="${CROSS_TRIPLE:-aarch64-nextui-linux-gnu}-gcc"
export CC_aarch64_unknown_linux_gnu="${CROSS_TRIPLE:-aarch64-nextui-linux-gnu}-gcc"
export CXX_aarch64_unknown_linux_gnu="${CROSS_TRIPLE:-aarch64-nextui-linux-gnu}-g++"
export AR_aarch64_unknown_linux_gnu="${CROSS_TRIPLE:-aarch64-nextui-linux-gnu}-ar"
command -v rustc >/dev/null 2>&1 || {
  echo "with-h700-toolchain.sh: rustc not found in the mounted rustup home" >&2
  echo "install toolchain 1.96 on the host (see rust-toolchain.toml) and retry" >&2
  exit 1
}
rustup target add aarch64-unknown-linux-gnu >/dev/null
'

echo "with-h700-toolchain.sh: $runtime run $image"
exec "$runtime" run --rm \
	-v "$repo_root:/src" \
	-v "$cargo_home:/root/.cargo" \
	-v "$rustup_home:/root/.rustup" \
	-e CARGO_HOME=/root/.cargo \
	-e RUSTUP_HOME=/root/.rustup \
	-e CARGO_BUILD_TARGET=aarch64-unknown-linux-gnu \
	-w /src \
	"$image" \
	/bin/bash -lc "$setup"$'\n'"$inner"
