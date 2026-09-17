#!/usr/bin/env bash
# Print the device slot binary produced by build:device, whether it was a native
# aarch64 build or a cross-compile via scripts/with-h700-toolchain.sh.
set -Eeuo pipefail

candidates=(
	target-device/aarch64-unknown-linux-gnu/device/slot
	target-device/device/slot
	target-device/aarch64-unknown-linux-gnu/release/slot
	target-device/release/slot
)

for path in "${candidates[@]}"; do
	if [[ -f "$path" ]]; then
		printf '%s\n' "$path"
		exit 0
	fi
done

echo "device-slot-bin.sh: no device slot binary under target-device/" >&2
exit 1
