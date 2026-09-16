# Third-party notices

Mac2Win Go itself is licensed under the MIT License (see `LICENSE`). Its
application bundle additionally contains third-party helper programs that keep
their own licenses. This file lists those components, the licenses they are
distributed under, and where to obtain their source code.

The helper programs are invoked by the application as separate processes. They
are aggregated with, not linked into, the Mac2Win Go binary.

## Bundled components

| Component | Version | License (SPDX) | Copyright holder | License text |
| --- | --- | --- | --- | --- |
| ntfs-3g: `mkntfs`, `ntfscat`, `ntfscp` | 2022.10.3 | `GPL-2.0-or-later` | Copyright (C) 2000-2011 Anton Altaparmakov; Copyright (C) Tuxera, Inc. and the ntfs-3g contributors | `licenses/GPL-2.0.txt` |
| ntfs-3g: `libntfs-3g.89.dylib` | 2022.10.3 | `LGPL-2.0-or-later` | Copyright (C) Tuxera, Inc. and the ntfs-3g contributors | `licenses/LGPL-2.0.txt` |
| wimlib: `wimlib-imagex`, `libwim.15.dylib` | 1.14.5 | `GPL-3.0-or-later` | Copyright (C) 2012-2024 Eric Biggers | `licenses/GPL-3.0.txt` |
| ms-sys | git (`jpz4085/ms-sys`) | `GPL-2.0-or-later` | Copyright (C) 2009-2015 Henrik Carlqvist | `licenses/GPL-2.0.txt` |
| GNU bash | 5.3 | `GPL-3.0-or-later` | Copyright (C) 1987-2024 Free Software Foundation, Inc. | `licenses/GPL-3.0.txt` |
| mtools: `mtools`, `mattrib` | 4.0.49 | `GPL-3.0-or-later` | Copyright (C) 1996-2024 Alain Knaff and the mtools contributors | `licenses/GPL-3.0.txt` |
| hivex: `hivexsh`, `libhivex.0.dylib` | 1.3.24 | `LGPL-2.1-only` | Copyright (C) 2009-2022 Red Hat Inc. | `licenses/LGPL-2.1.txt` |
| readpe: `peres`, `pev-plugins/*` | 0.85.1 | `GPL-2.0-or-later` | Copyright (C) 2012-2020 pev authors | `licenses/GPL-2.0.txt` |
| readpe: `libpe.0.85.dylib` | 0.85.1 | `LGPL-3.0-or-later` | Copyright (C) 2012-2020 pev authors | `licenses/LGPL-3.0.txt` |
| BCD-SYS (`bcd-sys`, `bcd-sys.d/`) | git (`jpz4085/BCD-SYS`) | `GPL-3.0-or-later` | Copyright (C) 2024 Joseph P. Zeller | `licenses/GPL-3.0.txt` |

`hiveset` is a Mac2Win Go helper licensed under MIT, but it is dynamically linked
against `libhivex` (LGPL-2.1-only). The dynamic link and the shared library
mechanism used by the bundle make it possible to relink `hiveset` against a
modified `libhivex`, as required by the LGPL. The `hiveset` binary is invoked as
a separate process, not linked into the application.

The bundled `libntfs-3g`, `libwim`, `libhivex` and `libpe` dylibs are private
copies placed in `Contents/Resources/tools`. Their load commands are rewritten
to `@loader_path` so the bundle is relocatable.

## Source code

The complete corresponding source code for the bundled helpers is available in
the Mac2Win Go repository at <https://github.com/vladislav-yashin/mac2wingo>
(see `licenses/SOURCE_OFFER.md`). `scripts/fetch-tools.sh` downloads the exact
upstream releases and applies the modifications described there and below.

Modifications made to the bundled helpers by this project:

- **BCD-SYS** — resolve the Windows volume by mount point, use the build-time
  pre-merged `Templates/BCD-Windows` and `Templates/BCD-Recovery` hives, and
  drop the runtime `hivexregedit` dependency.
- **readpe / libpe** — disable the OpenSSL-backed hashing module with an EVP
  shim and stubs so `peres` links without OpenSSL.
- **hivex** — adjust the `configure` linker map flag check for current clang,
  and stop requiring the test-only `IO::Stringy` Perl module so the Perl
  bindings (needed at build time for `hivexregedit`) are built with a stock
  Perl that does not ship Apple's system-Perl Extras.

## Rust dependencies

The Rust crates used by the application are not listed here; they are permissive
(MIT/Apache-2.0) and are not redistributed as separate files in the bundle.
