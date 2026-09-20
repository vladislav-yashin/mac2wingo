#!/usr/bin/env bash
#
# Sign and notarize the Mac2Win Go app and DMG with a Developer ID certificate.
#
# A Developer ID signature with hardened runtime plus Apple notarization is the
# only way to make the app open without a Gatekeeper warning. This script signs
# every nested helper binary (inside-out, no deprecated --deep), signs the bundle
# itself, submits it to Apple's notary service and staples the ticket.
#
# Without a signing identity it degrades to ad-hoc signing, which is enough for
# local development but still triggers the Gatekeeper warning once downloaded.
#
# Usage:
#   scripts/sign-and-notarize.sh --app [--skip-notarize]
#   scripts/sign-and-notarize.sh --dmg [--skip-notarize]
#
# Environment:
#   MAC2WINGO_SIGN_IDENTITY   "Developer ID Application: Name (TEAMID)".
#                             When empty, ad-hoc signing is used.
#   MAC2WINGO_ENTITLEMENTS    Override the entitlements plist path.
#   APPLE_API_KEY             Path to the App Store Connect .p8 key, its
#                             raw PEM contents, or its base64 encoding.
#   APPLE_API_KEY_ID          API key id.
#   APPLE_API_ISSUER          API issuer id (UUID).
#   APPLE_ID                  Apple account email (alternative to the API key).
#   APPLE_TEAM_ID             Team id (with APPLE_ID).
#   APPLE_APP_SPECIFIC_PASSWORD
#   NOTARY_TIMEOUT            notarytool --timeout (default: 30m).
#
# The default paths match what scripts/build-universal.sh and build-dmg.sh
# produce; override APP_PATH / DMG_PATH to sign something else.
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

APP_NAME="Mac2Win Go"
APP_PATH="${APP_PATH:-$ROOT_DIR/dist/$APP_NAME.app}"
DMG_PATH="${DMG_PATH:-}"

IDENTITY="${MAC2WINGO_SIGN_IDENTITY:-}"
ENTITLEMENTS="${MAC2WINGO_ENTITLEMENTS:-$ROOT_DIR/scripts/entitlements.plist}"
NOTARY_TIMEOUT="${NOTARY_TIMEOUT:-30m}"

TMP_DIR=""
cleanup() {
    [ -n "$TMP_DIR" ] && rm -rf "$TMP_DIR"
}
trap cleanup EXIT

log() { printf '\033[1;36m==> %s\033[0m\n' "$*"; }
warn() { printf '\033[1;33mwarning: %s\033[0m\n' "$*" >&2; }
die() {
    printf '\033[1;31merror: %s\033[0m\n' "$*" >&2
    exit 1
}

is_macho() {
    case "$(file -b "$1" 2>/dev/null || true)" in
        *Mach-O*) return 0 ;;
        *) return 1 ;;
    esac
}

# Sign every Mach-O file under the app's bundled helper tools, deepest first.
sign_nested() {
    local app="$1"
    local tools="$app/Contents/Resources/tools"
    [ -d "$tools" ] || return 0

    log "Signing nested helper binaries"
    local file
    # Libraries before executables, symlinks and shell scripts are skipped.
    while IFS= read -r file; do
        is_macho "$file" || continue
        codesign --force --options runtime --timestamp --sign "$IDENTITY" "$file" >/dev/null
    done < <(find "$tools" -type f \( -name '*.dylib' -o -perm -111 \) | sort)
}

sign_app() {
    local app="$1"

    if [ -z "$IDENTITY" ]; then
        warn "MAC2WINGO_SIGN_IDENTITY is not set; falling back to ad-hoc signing"
        warn "the resulting app will still be blocked by Gatekeeper when downloaded"
        codesign --force --deep --sign - "$app" >/dev/null
        return 0
    fi

    [ -f "$ENTITLEMENTS" ] || die "entitlements file not found: $ENTITLEMENTS"

    sign_nested "$app"

    log "Signing the app bundle"
    codesign --force --options runtime --timestamp \
        --entitlements "$ENTITLEMENTS" \
        --sign "$IDENTITY" "$app"

    log "Verifying the signature"
    codesign --verify --deep --strict --verbose=2 "$app"
}

sign_dmg() {
    local dmg="$1"
    if [ -z "$IDENTITY" ]; then
        warn "no signing identity; leaving the DMG ad-hoc/unsigned"
        return 0
    fi
    log "Signing the DMG"
    codesign --force --timestamp --sign "$IDENTITY" "$dmg"
}

# Populate NOTARY_ARGS with the credentials notarytool should use.
NOTARY_ARGS=()
prepare_notary_credentials() {
    NOTARY_ARGS=()

    if [ -n "${APPLE_API_KEY:-}" ]; then
        local key_path="$APPLE_API_KEY"
        if [ ! -f "$key_path" ]; then
            TMP_DIR="$(mktemp -d)"
            if printf '%s' "$APPLE_API_KEY" | grep -q 'BEGIN PRIVATE KEY'; then
                key_path="$TMP_DIR/AuthKey.p8"
                printf '%s\n' "$APPLE_API_KEY" > "$key_path"
            else
                key_path="$TMP_DIR/AuthKey.p8"
                printf '%s' "$APPLE_API_KEY" | base64 -D > "$key_path" 2>/dev/null \
                    || die "APPLE_API_KEY is neither a file nor valid base64/PEM"
            fi
        fi
        [ -n "${APPLE_API_KEY_ID:-}" ] || die "APPLE_API_KEY_ID is required with APPLE_API_KEY"
        [ -n "${APPLE_API_ISSUER:-}" ] || die "APPLE_API_ISSUER is required with APPLE_API_KEY"
        NOTARY_ARGS=(--key "$key_path" --key-id "$APPLE_API_KEY_ID" --issuer "$APPLE_API_ISSUER")
        return 0
    fi

    if [ -n "${APPLE_ID:-}" ]; then
        [ -n "${APPLE_TEAM_ID:-}" ] || die "APPLE_TEAM_ID is required with APPLE_ID"
        [ -n "${APPLE_APP_SPECIFIC_PASSWORD:-}" ] \
            || die "APPLE_APP_SPECIFIC_PASSWORD is required with APPLE_ID"
        NOTARY_ARGS=(--apple-id "$APPLE_ID" --team-id "$APPLE_TEAM_ID" \
            --password "$APPLE_APP_SPECIFIC_PASSWORD")
        return 0
    fi

    return 1
}

notarize() {
    local artifact="$1"
    local kind="$2"

    prepare_notary_credentials || die "no notarization credentials found (set APPLE_API_KEY/… or APPLE_ID/…)"

    local zip="$artifact"
    if [ "$kind" = "app" ]; then
        TMP_DIR="${TMP_DIR:-$(mktemp -d)}"
        zip="$TMP_DIR/$APP_NAME.zip"
        log "Packaging the app for notarization"
        ditto -c -k --keepParent "$artifact" "$zip"
    fi

    log "Submitting $kind for notarization (this can take a few minutes)"
    local out
    if ! out="$(xcrun notarytool submit "$zip" --wait --timeout "$NOTARY_TIMEOUT" \
        "${NOTARY_ARGS[@]}" 2>&1)"; then
        printf '%s\n' "$out" >&2
        die "notarytool failed"
    fi
    printf '%s\n' "$out"

    if ! printf '%s' "$out" | grep -q 'status: Accepted'; then
        die "notarization was not accepted; see the log above (xcrun notarytool log <id>)"
    fi

    log "Stapling the ticket to the $kind"
    xcrun stapler staple "$artifact"
    xcrun stapler validate "$artifact"
}

verify_app() {
    local app="$1"
    log "Gatekeeper assessment"
    spctl -a -vvv --type execute "$app" || warn "spctl rejected the app"
}

MODE=""
SKIP_NOTARIZE=0
while [ "$#" -gt 0 ]; do
    case "$1" in
        --app) MODE="app" ;;
        --dmg) MODE="dmg" ;;
        --skip-notarize) SKIP_NOTARIZE=1 ;;
        *) die "unknown argument: $1" ;;
    esac
    shift
done

case "$MODE" in
    app)
        [ -d "$APP_PATH" ] || die "app not found: $APP_PATH"
        sign_app "$APP_PATH"
        if [ "$SKIP_NOTARIZE" = "1" ] || [ -z "$IDENTITY" ]; then
            [ "$SKIP_NOTARIZE" = "1" ] && log "Skipping notarization (--skip-notarize)"
            [ -z "$IDENTITY" ] && log "Skipping notarization (ad-hoc build)"
        else
            notarize "$APP_PATH" app
        fi
        [ -n "$IDENTITY" ] && verify_app "$APP_PATH"
        log "Done: $APP_PATH"
        ;;
    dmg)
        [ -n "$DMG_PATH" ] || die "DMG_PATH is not set"
        [ -f "$DMG_PATH" ] || die "DMG not found: $DMG_PATH"
        sign_dmg "$DMG_PATH"
        if [ "$SKIP_NOTARIZE" = "1" ] || [ -z "$IDENTITY" ]; then
            log "Skipping DMG notarization"
        else
            notarize "$DMG_PATH" dmg
        fi
        log "Done: $DMG_PATH"
        ;;
    *)
        die "usage: $0 (--app | --dmg) [--skip-notarize]"
        ;;
esac
