# Source code offer

Mac2Win Go ships prebuilt helper programs under the GNU General Public License
(GPL) and the GNU Lesser General Public License (LGPL) inside its application
bundle. This file fulfils the corresponding-source obligations of those
licenses.

## Where to get the source

The complete corresponding source code for the bundled helper tools is available
in the Mac2Win Go source repository:

    https://github.com/vladislav-yashin/mac2wingo

The repository contains:

- `scripts/fetch-tools.sh` — downloads the exact upstream source releases listed
  below and builds them into `vendor/tools`;
- `scripts/hiveset.c` — the source of the small `hiveset` helper;
- every patch applied to the upstream sources (see below).

Running `./scripts/fetch-tools.sh` from a source checkout reproduces the exact
helper binaries that are copied into the app bundle, so the source in the
repository is the corresponding source for the distributed binaries.

## Upstream versions

| Helper | Version | Upstream |
| --- | --- | --- |
| ntfs-3g (`mkntfs`, `ntfscat`, `ntfscp`, `libntfs-3g`) | 2022.10.3 | https://github.com/tuxera/ntfs-3g |
| wimlib (`wimlib-imagex`, `libwim`) | 1.14.5 | https://wimlib.net |
| ms-sys | git `jpz4085/ms-sys` | https://github.com/jpz4085/ms-sys |
| GNU bash | 5.3 | https://ftp.gnu.org/gnu/bash |
| mtools | 4.0.49 | https://ftp.gnu.org/gnu/mtools |
| hivex (`hivexsh`, `libhivex`) | 1.3.24 | https://download.libguestfs.org/hivex |
| readpe / libpe (`peres`, `libpe`) | 0.85.1 | https://github.com/mentebinaria/readpe |
| BCD-SYS | git `jpz4085/BCD-SYS` | https://github.com/jpz4085/BCD-SYS |

## Modifications

The bundled binaries are not always pristine upstream builds. The following
modifications are applied by `scripts/fetch-tools.sh`, which is itself the
preferred form of these changes:

- **BCD-SYS** — `patch_bcd_sys` resolves the Windows volume by mount point,
  points the boot store generation at the pre-merged `Templates/BCD-Windows` and
  `Templates/BCD-Recovery` hives, and removes the runtime dependency on
  `hivexregedit` (the merge is performed at build time).
- **readpe / libpe** — OpenSSL-dependent hashing is disabled with a small EVP
  shim and stub implementations, so `peres` links without OpenSSL.
- **hivex** — the `configure` check for a linker map flag is adjusted so that it
  builds with current clang.

If you redistribute the app bundle, keep this offer and the license texts in
`licenses/` alongside the binaries.
