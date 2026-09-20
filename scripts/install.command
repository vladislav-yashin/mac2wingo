#!/bin/bash
# SPDX-License-Identifier: MIT
# Copyright (c) 2026 Vladislav Yashin
#
# Best-effort installer for the un-notarized build of Mac2Win Go.
#
# macOS Gatekeeper blocks the app when the downloaded DMG carries the
# com.apple.quarantine attribute (the "Apple cannot check ... for malicious
# software" alert). This helper copies the app to /Applications and clears the
# quarantine flag, after which it launches like any other app. It is a
# convenience for users who trust this build; the signed and notarized DMG does
# not need it.
#
# The script is shipped inside the DMG next to "Mac2Win Go.app".
set -euo pipefail

APP_NAME="Mac2Win Go.app"
here="$(cd "$(dirname "$0")" && pwd)"
src="$here/$APP_NAME"
dest="/Applications/$APP_NAME"

pause_and_exit() {
    printf '\n%s\n' "$1"
    printf 'Press Return to close this window. '
    read -r _ || true
    exit "${2:-0}"
}

if [ ! -d "$src" ]; then
    pause_and_exit "error: \"$APP_NAME\" was not found next to this script." 1
fi

printf '\nMac2Win Go - removing the Gatekeeper quarantine and installing to /Applications\n\n'

# Quit a running copy so it can be replaced safely.
if pgrep -x mac2wingo >/dev/null 2>&1; then
    printf 'Quitting the running Mac2Win Go...\n'
    osascript -e 'quit app "Mac2Win Go"' >/dev/null 2>&1 || pkill -x mac2wingo >/dev/null 2>&1 || true
    sleep 1
fi

if [ -e "$dest" ]; then
    printf 'Removing the previous copy...\n'
    rm -rf "$dest"
fi

printf 'Copying to /Applications...\n'
if ! ditto "$src" "$dest"; then
    pause_and_exit "error: could not copy the app to /Applications.
Drag \"$APP_NAME\" there manually, then run:
  xattr -dr com.apple.quarantine \"$dest\"" 1
fi

# Drop the quarantine flag (and any other extended attributes) so Gatekeeper no
# longer blocks the app on launch.
xattr -cr "$dest" 2>/dev/null || true
if xattr -p com.apple.quarantine "$dest" >/dev/null 2>&1; then
    printf '\nwarning: the quarantine flag is still set; macOS may ask for confirmation on first launch.\n'
fi

printf 'Launching Mac2Win Go...\n'
open "$dest" >/dev/null 2>&1 || true

pause_and_exit "Done. Mac2Win Go is now in your Applications folder."
