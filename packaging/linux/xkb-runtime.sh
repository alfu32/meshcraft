#!/bin/sh
# The X11 bridge uses private core-library structures. Never mix runtime pairs.
# Return success when the ordinary loader search would choose an incomplete or
# split pair. A subshell preserves the caller's IFS and globbing configuration.
meshcraft_needs_bundled_xkb() (
    meshcraft_core_dir=
    meshcraft_bridge_dir=
    IFS=:
    set -f
    for meshcraft_directory in $1; do
        [ -n "$meshcraft_directory" ] || continue
        if [ -z "$meshcraft_core_dir" ] && [ -f "$meshcraft_directory/libxkbcommon.so.0" ]; then
            meshcraft_core_dir=$meshcraft_directory
        fi
        if [ -z "$meshcraft_bridge_dir" ] && [ -f "$meshcraft_directory/libxkbcommon-x11.so.0" ]; then
            meshcraft_bridge_dir=$meshcraft_directory
        fi
    done
    [ -z "$meshcraft_core_dir" ] || [ "$meshcraft_core_dir" != "$meshcraft_bridge_dir" ]
)
