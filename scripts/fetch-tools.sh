#!/usr/bin/env bash
#
# Build the helper binaries that are shipped inside «Mac2Win Go.app».
#
# The app must work out of the box, so the following universal (arm64 + x86_64)
# helpers are placed in vendor/tools:
#
#   * wimlib-imagex  - built with --with-ntfs-3g so it can apply a WIM directly
#                      to a raw NTFS partition (no macFUSE needed);
#   * mkntfs         - from ntfs-3g (creates NTFS without a kernel driver);
#   * ms-sys         - writes legacy MBR boot code (optional, BIOS only);
#   * bcd-sys        - BCD-SYS helper bundle (copies boot files, writes BCD).
#
# BCD-SYS itself shells out to bash (>= 4), hivex (hivexsh), mtools (mattrib)
# and readpe (peres), plus it merges .reg templates with hivexregedit. None of
# those may be assumed to exist on the target machine, so they are built here
# as well:
#
#   * bash           - GNU bash 5 (macOS ships 3.2);
#   * hivex          - hivexsh plus, at build time only, hivexregedit to
#                      pre-merge the BCD templates; also builds `hiveset`, a
#                      tiny libhivex helper that edits single registry values;
#   * mtools         - mtools + mattrib;
#   * peres          - PE resource dump from readpe (no OpenSSL required).
#
# The upstream GPL/LGPL license texts of all of the above are collected into
# vendor/tools/licenses so they ship inside the app bundle. The modifications
# this script applies to the upstream sources are documented in
# THIRD_PARTY_NOTICES.md and licenses/SOURCE_OFFER.md.
#
# Usage:
#   ./scripts/fetch-tools.sh            # build everything
#   FETCH_FORCE=1 ./scripts/fetch-tools.sh
#
# This is a build-time helper: it needs Xcode command line tools and network
# access. Regular `cargo test` does not need it.
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
VENDOR_DIR="$ROOT_DIR/vendor/tools"
WORK_DIR="${WORK_DIR:-$ROOT_DIR/.tools-build}"
PREFIX="$WORK_DIR/prefix"

NTFS3G_VERSION="${NTFS3G_VERSION:-2022.10.3}"
WIMLIB_VERSION="${WIMLIB_VERSION:-1.14.5}"
BASH_RELEASE="${BASH_RELEASE:-5.3}"
MTOOLS_VERSION="${MTOOLS_VERSION:-4.0.49}"
HIVEX_VERSION="${HIVEX_VERSION:-1.3.24}"
READPE_VERSION="${READPE_VERSION:-0.85.1}"
MS_SYS_REPO="${MS_SYS_REPO:-https://github.com/jpz4085/ms-sys.git}"
BCD_SYS_REPO="${BCD_SYS_REPO:-https://github.com/jpz4085/BCD-SYS.git}"

# Compile both architectures in a single pass; clang emits universal binaries.
UNIVERSAL=(-arch arm64 -arch x86_64)
export CFLAGS="${CFLAGS:-} -O2 ${UNIVERSAL[*]}"
export CXXFLAGS="$CFLAGS"
export LDFLAGS="${UNIVERSAL[*]}"
export CPPFLAGS="-I$PREFIX/include ${CPPFLAGS:-}"
export PKG_CONFIG_PATH="$PREFIX/lib/pkgconfig"

JOBS="$(sysctl -n hw.ncpu 2>/dev/null || echo 4)"

log() { printf '\033[1;36m==> %s\033[0m\n' "$*"; }
die() { printf '\033[1;31merror: %s\033[0m\n' "$*" >&2; exit 1; }

require_macos() {
    [ "$(uname -s)" = "Darwin" ] || die "helper tools can only be built on macOS"
    command -v clang >/dev/null 2>&1 || die "Xcode command line tools are required (clang)"
    command -v curl >/dev/null 2>&1 || die "curl is required"
}

# ntfs-3g's source archive ships only configure.ac, so GNU autotools are
# needed to generate configure. macOS (and GitHub's runners) do not provide
# them by default.
require_autotools() {
    local missing=()
    local tool
    for tool in autoreconf aclocal automake; do
        command -v "$tool" >/dev/null 2>&1 || missing+=("$tool")
    done
    if ! command -v libtoolize >/dev/null 2>&1 && ! command -v glibtoolize >/dev/null 2>&1; then
        missing+=(libtool)
    fi
    if [ "${#missing[@]}" -gt 0 ]; then
        die "missing build tools: ${missing[*]} (install with: brew install autoconf automake libtool)"
    fi
}

download() {
    local url="$1" out="$2"
    [ -f "$out" ] && return 0
    log "downloading $(basename "$out")"
    curl --fail --location --retry 3 --output "$out" "$url"
}

build_ntfs3g() {
    [ -x "$VENDOR_DIR/mkntfs" ] && return 0
    require_autotools
    log "building ntfs-3g $NTFS3G_VERSION (library + ntfsprogs)"
    local src="$WORK_DIR/ntfs-3g-$NTFS3G_VERSION"
    if [ ! -d "$src" ]; then
        download \
            "https://github.com/tuxera/ntfs-3g/archive/refs/tags/$NTFS3G_VERSION.tar.gz" \
            "$WORK_DIR/ntfs-3g.tar.gz"
        tar -xzf "$WORK_DIR/ntfs-3g.tar.gz" -C "$WORK_DIR"
    fi
    (
        cd "$src"
        autoreconf --install --force
        [ -x ./configure ] || die "autoreconf did not generate ./configure for ntfs-3g"
        ./configure \
            --prefix="$PREFIX" \
            --disable-ntfs-3g \
            --enable-ntfsprogs \
            --disable-ldconfig \
            --disable-mount-helper
        make -j"$JOBS"
        # On macOS ntfs-3g's install-exec-hook tries to move the (non-existent)
        # libntfs-3g.so* into /lib and aborts the install. Pointing rootlibdir at
        # the prefix makes that hook a no-op so the dylib stays in $PREFIX/lib.
        make install rootlibdir="$PREFIX/lib"
    )
    # mkntfs and ntfscp live in sbin; ntfscat lives in bin. The offline
    # Windows To Go configuration extracts, edits and writes back the SYSTEM and
    # SOFTWARE registry hives directly on the unmounted NTFS partition.
    cp "$PREFIX/sbin/mkntfs" "$VENDOR_DIR/mkntfs"
    cp "$PREFIX/sbin/ntfscp" "$VENDOR_DIR/ntfscp"
    cp "$PREFIX/bin/ntfscat" "$VENDOR_DIR/ntfscat"
}

build_wimlib() {
    [ -x "$VENDOR_DIR/wimlib-imagex" ] && return 0
    log "building wimlib $WIMLIB_VERSION (--with-ntfs-3g)"
    local src="$WORK_DIR/wimlib-$WIMLIB_VERSION"
    if [ ! -d "$src" ]; then
        download \
            "https://wimlib.net/downloads/wimlib-$WIMLIB_VERSION.tar.gz" \
            "$WORK_DIR/wimlib.tar.gz"
        tar -xzf "$WORK_DIR/wimlib.tar.gz" -C "$WORK_DIR"
    fi
    (
        cd "$src"
        ./configure --prefix="$PREFIX" --with-ntfs-3g --without-fuse
        make -j"$JOBS"
        make install
    )
    cp "$PREFIX/bin/wimlib-imagex" "$VENDOR_DIR/wimlib-imagex"
}

build_ms_sys() {
    [ -x "$VENDOR_DIR/ms-sys" ] && return 0
    log "building ms-sys (universal)"
    local src="$WORK_DIR/ms-sys"
    [ -d "$src" ] || git clone --depth 1 "$MS_SYS_REPO" "$src"
    # ms-sys generates dependencies with `clang -MM`, which errors out when more
    # than one -arch is passed, so build each slice separately and lipo them.
    for arch in arm64 x86_64; do
        (
            export CFLAGS="-O2 -arch $arch"
            export CXXFLAGS="$CFLAGS"
            export LDFLAGS="-arch $arch"
            make -C "$src" clean >/dev/null 2>&1 || true
            make -C "$src" -j"$JOBS" build/bin/ms-sys
        )
        # Keep the slices outside build/bin: `make clean` wipes that directory.
        cp "$src/build/bin/ms-sys" "$WORK_DIR/ms-sys-$arch"
    done
    lipo -create \
        "$WORK_DIR/ms-sys-arm64" \
        "$WORK_DIR/ms-sys-x86_64" \
        -output "$VENDOR_DIR/ms-sys"
}

build_bash() {
    [ -x "$VENDOR_DIR/bash" ] && return 0
    log "building bash $BASH_RELEASE"
    local src="$WORK_DIR/bash-$BASH_RELEASE"
    if [ ! -d "$src" ]; then
        download \
            "https://ftp.gnu.org/gnu/bash/bash-$BASH_RELEASE.tar.gz" \
            "$WORK_DIR/bash.tar.gz"
        tar -xzf "$WORK_DIR/bash.tar.gz" -C "$WORK_DIR"
    fi
    (
        cd "$src"
        # Build bash's own readline against the system ncurses so we do not
        # depend on Homebrew's readline/ncurses.
        ./configure --prefix="$PREFIX" --without-bash-malloc --with-curses
        make -j"$JOBS"
        make install
    )
    cp "$PREFIX/bin/bash" "$VENDOR_DIR/bash"
}

build_mtools() {
    [ -x "$VENDOR_DIR/mattrib" ] && return 0
    log "building mtools $MTOOLS_VERSION"
    local src="$WORK_DIR/mtools-$MTOOLS_VERSION"
    if [ ! -d "$src" ]; then
        download \
            "https://ftp.gnu.org/gnu/mtools/mtools-$MTOOLS_VERSION.tar.gz" \
            "$WORK_DIR/mtools.tar.gz"
        tar -xzf "$WORK_DIR/mtools.tar.gz" -C "$WORK_DIR"
    fi
    (
        cd "$src"
        # mtools mis-detects stat64 on Apple Silicon and needs libiconv on macOS.
        ac_cv_func_stat64=no ./configure \
            --prefix="$PREFIX" \
            --sysconfdir="$PREFIX/etc" \
            --without-x \
            LIBS=-liconv
        make -j"$JOBS"
        make install
    )
    cp "$PREFIX/bin/mtools" "$VENDOR_DIR/mtools"
    cp "$PREFIX/bin/mattrib" "$VENDOR_DIR/mattrib"
}

build_hivex() {
    [ -x "$VENDOR_DIR/hivexsh" ] && return 0
    log "building hivex $HIVEX_VERSION"
    local src="$WORK_DIR/hivex-$HIVEX_VERSION"
    if [ ! -d "$src" ]; then
        download \
            "https://download.libguestfs.org/hivex/hivex-$HIVEX_VERSION.tar.gz" \
            "$WORK_DIR/hivex.tar.gz"
        tar -xzf "$WORK_DIR/hivex.tar.gz" -C "$WORK_DIR"
    fi
    (
        cd "$src"
        # Newer clang rejects the linker map flag the configure check uses.
        perl -pi -e 's/\Q-Wl,-M -Wl,\E/-Wl,-map -Wl,/' configure
        # configure refuses to build the Perl bindings unless IO::Stringy is
        # installed, but that module is only used by hivex's test suite (not by
        # hivexregedit or Win::Hivex). Apple ships it in the system Perl's
        # "Extras", while a stock/Homebrew Perl does not, so require only the
        # modules actually needed to build the bindings.
        perl -pi -e 's/for pm in Test::More ExtUtils::MakeMaker IO::Stringy; do/for pm in Test::More ExtUtils::MakeMaker; do/' configure
        # Perl bindings stay enabled: hivexregedit is needed once, at build
        # time, to pre-merge the BCD templates. They are not shipped.
        ./configure --prefix="$PREFIX" \
            --disable-ocaml --disable-python --disable-ruby \
            --without-readline --disable-nls --disable-silent-rules
        make -j"$JOBS"
        make install
    )
    [ -x "$PREFIX/bin/hivexregedit" ] || die "hivex did not build its Perl bindings (hivexregedit)"
    cp "$PREFIX/bin/hivexsh" "$VENDOR_DIR/hivexsh"
}

# `hiveset` edits a single value in an offline hive. `hivexsh` can only replace
# *all* values at a node, which would wipe shared keys like \ControlSet001\Control,
# so the offline Windows To Go configuration needs this non-destructive helper.
build_hiveset() {
    [ -x "$VENDOR_DIR/hiveset" ] && return 0
    log "building hiveset (libhivex helper)"
    clang -O2 "${UNIVERSAL[@]}" \
        -I"$PREFIX/include" \
        -o "$VENDOR_DIR/hiveset" \
        "$ROOT_DIR/scripts/hiveset.c" \
        -L"$PREFIX/lib" -lhivex
}

build_readpe() {
    [ -x "$VENDOR_DIR/peres" ] && return 0
    log "building readpe $READPE_VERSION (peres only)"
    local src="$WORK_DIR/readpe-$READPE_VERSION"
    if [ ! -d "$src" ]; then
        download \
            "https://github.com/mentebinaria/readpe/archive/refs/tags/v$READPE_VERSION.tar.gz" \
            "$WORK_DIR/readpe.tar.gz"
        tar -xzf "$WORK_DIR/readpe.tar.gz" -C "$WORK_DIR"
    fi

    # libpe links OpenSSL but `peres` never uses it (only pehash/pesec do).
    # Disable the OpenSSL-backed hashing module and provide a tiny EVP shim so
    # the library links without OpenSSL at all.
    local pe="$src/lib/libpe"
    mkdir -p "$pe/include/openssl"
    cat > "$pe/include/openssl/evp.h" <<'EOF'
/* Minimal OpenSSL EVP shim for the `peres` build of libpe. */
#ifndef LIBPE_OPENSSL_EVP_SHIM_H
#define LIBPE_OPENSSL_EVP_SHIM_H
#ifdef __cplusplus
extern "C" {
#endif
void CRYPTO_cleanup_all_ex_data(void);
void OpenSSL_add_all_digests(void);
void EVP_cleanup(void);
#ifdef __cplusplus
}
#endif
#endif
EOF
    cat > "$pe/include/openssl/md5.h" <<'EOF'
#ifndef LIBPE_OPENSSL_MD5_SHIM_H
#define LIBPE_OPENSSL_MD5_SHIM_H
#include "evp.h"
#endif
EOF
    cat > "$pe/openssl_compat.c" <<'EOF'
/* No-op OpenSSL cleanup hooks; see include/openssl/evp.h. */
void CRYPTO_cleanup_all_ex_data(void) {}
void OpenSSL_add_all_digests(void) {}
void EVP_cleanup(void) {}
EOF
    cat > "$pe/hashes_stub.c" <<'EOF'
/* Stubs replacing libpe's OpenSSL-backed hashing module. */
#include "libpe/hashes.h"
#include <stddef.h>

void pe_hash_headers_dealloc(pe_hash_headers_t *obj) { (void)obj; }
void pe_hash_sections_dealloc(pe_hash_sections_t *obj) { (void)obj; }
void pe_hash_dealloc(pe_hash_t *obj) { (void)obj; }
EOF
    [ -f "$pe/hashes.c" ] && mv "$pe/hashes.c" "$pe/hashes.c.disabled"

    perl -pi -e 's/-lssl -lcrypto -lm/-lm/' "$pe/Makefile"
    perl -pi -e 's/\s-flat_namespace//' "$pe/Makefile"
    perl -pi -e 's/-L\$\(LIBPE\) -lpe -lcrypto -lssl -ldl -lm/-L\$(LIBPE) -lpe -lm/' "$src/src/Makefile"
    perl -0pi -e 's/peres: \$\(pev_BUILDDIR\)\/peres\.o \$\(pev_OBJS\)/peres: \$(pev_BUILDDIR)\/peres.o \$(pev_COMMON_DEPS)/' "$src/src/Makefile"
    # Let the bundled binary find its output plugins next to the executable.
    perl -0pi -e 's/if \(config->plugins_path == NULL\)\n        config->plugins_path = strdup\(DEFAULT_PLUGINS_PATH\);/if (config->plugins_path == NULL) {\n        const char *env_plugins = getenv("PEV_PLUGINS_DIR");\n        config->plugins_path = strdup((env_plugins && *env_plugins) ? env_plugins : DEFAULT_PLUGINS_PATH);\n    }/' "$src/src/config.c"

    (
        cd "$src"
        make -C lib/libpe CC=clang prefix="$PREFIX" libdir="$PREFIX/lib"
        make -C lib/libpe CC=clang prefix="$PREFIX" libdir="$PREFIX/lib" install
        make -C src CC=clang peres
        make -C src CC=clang plugins
    )
    cp "$src/src/build/peres" "$VENDOR_DIR/peres"
    mkdir -p "$VENDOR_DIR/pev-plugins"
    cp "$src/src/build/plugins/"*_plugin.dylib "$VENDOR_DIR/pev-plugins/"
}

# Pre-merge the static .reg templates into the BCD hive at build time, so the
# shipped bcd-sys does not need hivexregedit (and therefore Perl) at runtime.
premerge_bcd_templates() {
    local dest="$1"
    local perl_lib
    local hivex_pm
    hivex_pm="$(find "$PREFIX/lib/perl5" -path '*/Win/Hivex.pm' -print -quit 2>/dev/null)"
    [ -x "$PREFIX/bin/hivexregedit" ] || die "hivexregedit is required to pre-merge the BCD templates"
    [ -n "$hivex_pm" ] || die "hivex Perl bindings were not installed"
    perl_lib="$(dirname "$(dirname "$hivex_pm")")"

    log "pre-merging BCD templates"
    local tpl="$dest/Templates"
    cp "$tpl/BCD-NEW" "$tpl/BCD-Windows"
    PERL5LIB="$perl_lib" "$PREFIX/bin/hivexregedit" \
        --merge --prefix BCD00000001 "$tpl/BCD-Windows" "$tpl/winload.reg"
    cp "$tpl/BCD-NEW" "$tpl/BCD-Recovery"
    PERL5LIB="$perl_lib" "$PREFIX/bin/hivexregedit" \
        --merge --prefix BCD00000001 "$tpl/BCD-Recovery" "$tpl/recovery.reg"
}

# Point bcd-sys at the pre-merged hives and drop the runtime hivexregedit use.
patch_bcd_sys() {
    local script="$1"
    # Resolve the Windows volume by its mount point rather than by the basename
    # of the mount point. macOS is case sensitive about volume names, so a
    # temporary mount point like `<tmp>/windows` makes `diskutil info windows`
    # fail even though the volume is labelled `WINDOWS`.
    perl -0pi -e 's/diskutil info "\$\(basename "\$winpath"\)"/diskutil info "\$winpath"/g' "$script"
    perl -0pi -e 's/cp \$resdir\/Templates\/BCD-NEW \$tmpdir\/BCD-Windows/cp \$resdir\/Templates\/BCD-Windows \$tmpdir\/BCD-Windows/' "$script"
    perl -ni -e 'print unless /hivexregedit --merge --prefix BCD00000001 \$tmpdir\/BCD-Windows/' "$script"
    perl -0pi -e 's/cp \$resdir\/Templates\/BCD-NEW \$tmpdir\/BCD-Recovery/cp \$resdir\/Templates\/BCD-Recovery \$tmpdir\/BCD-Recovery/' "$script"
    perl -ni -e 'print unless /hivexregedit --merge --prefix BCD00000001 \$tmpdir\/BCD-Recovery/' "$script"
    perl -ni -e 'print unless /command -v hivexregedit/' "$script"
}

fetch_bcd_sys() {
    log "fetching BCD-SYS"
    local src="$WORK_DIR/BCD-SYS"
    [ -d "$src" ] || git clone --depth 1 "$BCD_SYS_REPO" "$src"
    local dest="$VENDOR_DIR/bcd-sys.d"
    rm -rf "$dest"
    mkdir -p "$dest"
    cp -R "$src/Darwin" "$dest/Darwin"
    cp -R "$src/Resources" "$dest/Resources"
    cp -R "$src/Templates" "$dest/Templates"
    chmod +x "$dest/Darwin/"*.sh
    # BCD-SYS must run from its `Darwin` directory and expects `Resources` and
    # `Templates` next to it. Create the links now so runtime never has to write
    # inside the (possibly signed) app bundle.
    ln -sfn ../Resources "$dest/Darwin/Resources"
    ln -sfn ../Templates "$dest/Darwin/Templates"

    premerge_bcd_templates "$dest"
    patch_bcd_sys "$dest/Darwin/bcd-sys.sh"

    # The wrapper exposes the bundled bash/hivex/mtools/peres on PATH so the
    # BCD-SYS script resolves them to our copies instead of the system ones.
    cat > "$VENDOR_DIR/bcd-sys" <<'EOF'
#!/bin/sh
# SPDX-License-Identifier: MIT
# Copyright (c) 2026 Vladislav Yashin
# Wrapper generated by scripts/fetch-tools.sh. It only locates and executes the
# bundled BCD-SYS (GPL-3.0-or-later) script, which keeps its own license.
here="$(cd "$(dirname "$0")" && pwd)"
export PATH="$here:$PATH"
export PEV_PLUGINS_DIR="$here/pev-plugins"
cd "$here/bcd-sys.d/Darwin" || exit 1
exec ./bcd-sys.sh "$@"
EOF
    chmod +x "$VENDOR_DIR/bcd-sys"
}

# Copy the third-party dylibs the tools link against into `vendor/tools` and
# rewrite every reference to use `@loader_path`, so the resulting bundle is
# relocatable and does not depend on the `.tools-build` tree (or Homebrew).
bundle_shared_libraries() {
    local prefix_lib="$PREFIX/lib"
    [ -d "$prefix_lib" ] || return 0
    [ -x "$(command -v install_name_tool)" ] || die "install_name_tool is required"

    log "bundling shared libraries"
    local file dep
    for file in "$prefix_lib"/*.dylib; do
        [ -f "$file" ] && [ ! -L "$file" ] || continue
        cp -f "$file" "$VENDOR_DIR/$(basename "$file")"
    done

    for file in "$VENDOR_DIR"/*.dylib; do
        [ -f "$file" ] || continue
        install_name_tool -id "@loader_path/$(basename "$file")" "$file"
        while IFS= read -r dep; do
            case "$dep" in
                "$prefix_lib"/*)
                    install_name_tool -change "$dep" "@loader_path/$(basename "$dep")" "$file" ;;
            esac
        done < <(otool -L "$file" | awk '/\.dylib/ {print $1}')
    done

    local tool
    for tool in "$VENDOR_DIR"/*; do
        [ -f "$tool" ] || continue
        case "$tool" in *.dylib) continue ;; esac
        case "$(file -b "$tool")" in *Mach-O*) ;; *) continue ;; esac
        while IFS= read -r dep; do
            case "$dep" in
                "$prefix_lib"/*)
                    install_name_tool -change "$dep" "@loader_path/$(basename "$dep")" "$tool" ;;
            esac
        done < <(otool -L "$tool" | awk '/\.dylib/ {print $1}')
    done

    # pev output plugins live one directory below the libraries they link.
    for file in "$VENDOR_DIR/pev-plugins"/*.dylib; do
        [ -f "$file" ] || continue
        while IFS= read -r dep; do
            case "$dep" in
                "$prefix_lib"/*)
                    install_name_tool -change "$dep" "@loader_path/../$(basename "$dep")" "$file" ;;
            esac
        done < <(otool -L "$file" | awk '/\.dylib/ {print $1}')
    done
}

# Collect the upstream license texts of the helpers. The GPL/LGPL require the
# license to be distributed together with the binaries, so `build-universal.sh`
# copies this directory into the app bundle.
bundle_licenses() {
    log "bundling license texts"
    local dest="$VENDOR_DIR/licenses"
    local component src name
    rm -rf "$dest"
    mkdir -p "$dest"

    copy_license() {
        component="$1"
        src="$2"
        name="${3:-$(basename "$src")}"
        [ -f "$src" ] || die "missing license file for $component: $src"
        mkdir -p "$dest/$component"
        cp "$src" "$dest/$component/$name"
    }

    copy_license ntfs-3g "$WORK_DIR/ntfs-3g-$NTFS3G_VERSION/COPYING" GPL-2.0.txt
    copy_license ntfs-3g "$WORK_DIR/ntfs-3g-$NTFS3G_VERSION/COPYING.LIB" LGPL-2.0.txt
    copy_license wimlib "$WORK_DIR/wimlib-$WIMLIB_VERSION/COPYING" COPYING
    copy_license wimlib "$WORK_DIR/wimlib-$WIMLIB_VERSION/COPYING.GPLv3" GPL-3.0.txt
    copy_license wimlib "$WORK_DIR/wimlib-$WIMLIB_VERSION/COPYING.LGPL" LGPL-2.1.txt
    copy_license ms-sys "$WORK_DIR/ms-sys/COPYING"
    copy_license bash "$WORK_DIR/bash-$BASH_RELEASE/COPYING"
    copy_license mtools "$WORK_DIR/mtools-$MTOOLS_VERSION/COPYING"
    copy_license hivex "$WORK_DIR/hivex-$HIVEX_VERSION/LICENSE"
    copy_license readpe "$WORK_DIR/readpe-$READPE_VERSION/LICENSE"
    copy_license readpe "$WORK_DIR/readpe-$READPE_VERSION/lib/libpe/LICENSE" LGPL-3.0.txt
    copy_license bcd-sys "$WORK_DIR/BCD-SYS/LICENSE"
    copy_license hiveset "$ROOT_DIR/LICENSE"
}

main() {
    require_macos
    mkdir -p "$VENDOR_DIR" "$WORK_DIR" "$PREFIX"
    build_ntfs3g
    build_wimlib
    build_ms_sys
    build_bash
    build_mtools
    build_hivex
    build_hiveset
    build_readpe
    fetch_bcd_sys
    bundle_shared_libraries
    bundle_licenses

    log "verifying architectures"
    for tool in mkntfs ntfscat ntfscp wimlib-imagex ms-sys bash mtools mattrib hivexsh hiveset peres; do
        if [ -x "$VENDOR_DIR/$tool" ]; then
            printf '  %-16s %s\n' "$tool" "$(lipo -archs "$VENDOR_DIR/$tool" 2>/dev/null || echo '?')"
        else
            printf '  %-16s (missing)\n' "$tool"
        fi
    done
    log "done: $VENDOR_DIR"
}

main "$@"
