# Bundled helper tools

This directory holds the universal macOS helper binaries that are copied into
`Mac2Win Go.app/Contents/Resources/tools` by `scripts/build-universal.sh`.

The binaries are **not** committed. Run:

```sh
./scripts/fetch-tools.sh
```

to build them from source. The application resolves the top-level tools with
`ToolLocator` and `bcd-sys` puts this directory on its own `PATH` for the rest.

Required at runtime:

| Tool | Purpose |
| --- | --- |
| `wimlib-imagex` | applies `install.wim`/`.esd` directly to a raw NTFS partition (built with `--with-ntfs-3g`) |
| `mkntfs` | creates the NTFS filesystem (no macFUSE) |
| `ntfscat` | reads `SYSTEM`/`SOFTWARE` hives off the unmounted NTFS partition |
| `hiveset` | sets a single value in an offline hive (non-destructive, unlike `hivexsh`'s `setval`) |
| `ntfscp` | writes the patched hives and `unattend.xml` back to the NTFS partition |
| `ms-sys` | legacy BIOS MBR boot code (optional, MBR scheme only) |
| `bcd-sys` | copies Windows boot files and generates the BCD store |
| `bash` | GNU bash 5, because macOS only ships 3.2 (used by BCD-SYS) |
| `hivexsh` | edits the BCD hive (used by BCD-SYS) |
| `mtools`, `mattrib` | FAT file attributes on the EFI partition (used by BCD-SYS) |
| `peres` | reads PE version resources (used by BCD-SYS) |
| `*.dylib` | private copies of `wimlib`, `ntfs-3g`, `hivex` and `libpe` |

Supporting files:

- `bcd-sys.d/` — the BCD-SYS scripts plus pre-merged `Templates/BCD-Windows`
  and `Templates/BCD-Recovery` hives (merged at build time, so the shipped app
  does **not** need `hivexregedit`/Perl at runtime).
- `pev-plugins/` — output plugins for `peres`, found through `PEV_PLUGINS_DIR`.
- `licenses/` — the upstream GPL/LGPL license texts of the tools above, copied
  by `fetch-tools.sh` so they ship inside the app bundle.

`ToolLocator` searches, in order: `$MAC2WINGO_TOOLS`, the app bundle resources,
this development directory, then `PATH`.
