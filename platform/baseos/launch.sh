#!/bin/sh

# BaseOS v1.1.0 mounts the active frontend volume here: TF2 when present,
# otherwise the data partition of the BaseOS card in TF1.
SD=/mnt/sdcard
export SLOT_ROOT="$SD"

# The H700's simple_ondemand governor reacts too late to short GLES bursts under VSync.
# NextUI pins the GPU floor to its highest advertised clock: power gating still idles the
# block between bursts, while every composition has the clock it needs before the VBlank.
for DEVFREQ in /sys/class/devfreq/*gpu*; do
	[ -w "$DEVFREQ/min_freq" ] || continue
	[ -r "$DEVFREQ/available_frequencies" ] || continue
	GPU_MAX=
	for FREQ in $(cat "$DEVFREQ/available_frequencies"); do
		case "$FREQ" in
			*[!0-9]*|'') continue ;;
		esac
		[ -n "$GPU_MAX" ] && [ "$FREQ" -le "$GPU_MAX" ] || GPU_MAX=$FREQ
	done
	[ -n "$GPU_MAX" ] && echo "$GPU_MAX" > "$DEVFREQ/min_freq"
done

cd "$SD/System" || exit 1
exec "$SD/System/slot" >> "$SD/System/slot.log" 2>&1
