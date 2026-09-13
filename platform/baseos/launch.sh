#!/bin/sh

# BaseOS v1.1.0 mounts the active frontend volume here: TF2 when present,
# otherwise the data partition of the BaseOS card in TF1.
SD=/mnt/sdcard
export SLOT_ROOT="$SD"

cd "$SD/System" || exit 1
exec "$SD/System/slot" >> "$SD/System/slot.log" 2>&1
