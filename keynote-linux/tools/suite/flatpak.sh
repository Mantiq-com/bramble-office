#!/bin/sh
# Wraps one built program into a single-file Flatpak bundle (<out>.flatpak), for people who
# install from a download instead of a store. Needs flatpak with org.freedesktop.Platform
# (the runtime below) installed; no flatpak-builder, no SDK.
#
#   flatpak.sh <binary> <app id> <command> <icons, with {s} for the size> <out.flatpak> [extra finish args...]
#
# The .desktop and metainfo files come from the repository's flatpak/ folder (named by app id).
# Run from the repository's top folder.
set -eu
RUNTIME=26.08
BIN=$1; ID=$2; CMD=$3; ICONS=$4; OUT=$5; shift 5
D=$(mktemp -d)
trap 'rm -rf "$D"' EXIT
flatpak build-init "$D/app" "$ID" org.freedesktop.Platform org.freedesktop.Platform "$RUNTIME"
install -Dm755 "$BIN" "$D/app/files/bin/$CMD"
install -Dm644 "flatpak/$ID.desktop" "$D/app/files/share/applications/$ID.desktop"
install -Dm644 "flatpak/$ID.metainfo.xml" "$D/app/files/share/metainfo/$ID.metainfo.xml"
for s in 16 24 32 48 64 128 256 512; do
    f=$(echo "$ICONS" | sed "s/{s}/$s/")
    [ -f "$f" ] && install -Dm644 "$f" "$D/app/files/share/icons/hicolor/${s}x${s}/apps/$ID.png"
done
flatpak build-finish "$D/app" --command="$CMD" --share=ipc --socket=wayland --socket=fallback-x11 --device=dri "$@"
flatpak build-export "$D/repo" "$D/app" stable
flatpak build-bundle "$D/repo" "$OUT" "$ID" stable --runtime-repo=https://dl.flathub.org/repo/flathub.flatpakrepo
ls -la "$OUT"
