# Portable / USB Edition

Run FreEco.ai from a USB drive (or any folder) with no installation and no
system changes — config, agent state, and data all live next to the binary
and travel with the drive between computers.

## For end users

1. Get a `freeco-portable` folder (from a teammate, a release download, or
   by building one yourself — see below).
2. Copy the folder onto a USB drive, or anywhere on disk.
3. Run the script for your OS:
   - **Windows**: double-click `run-windows.bat`
   - **macOS**: double-click `run-macos.command` (first run: right-click →
     Open, to get past Gatekeeper's "unidentified developer" warning)
   - **Linux**: double-click `run-linux.sh`, or `./run-linux.sh` from a
     terminal if your file manager opens `.sh` files as text instead of
     running them
4. On first run, a setup wizard asks for an LLM provider API key. After
   that, the dashboard opens automatically at `http://127.0.0.1:4200`.

Everything is written to the `data/` folder inside the bundle — nothing
touches the host machine's home directory or registry. To move to a new
computer, just copy the whole folder.

## Building the bundle

```bash
scripts/build-portable.sh            # packages the latest release
scripts/build-portable.sh v0.6.9      # or a specific version
```

This downloads the Windows, macOS, and Linux CLI binaries (x86_64 and
aarch64) from GitHub Releases, verifies their checksums, and assembles them
with the launcher scripts from `scripts/portable/` into `dist/freeco-portable/`,
ready to copy onto a drive.

## How it works

The launchers set two environment variables before starting `openfang`:

- `OPENFANG_HOME` — pointed at `data/` next to the script, relocating
  config, the local database, agent workspaces, and logs there instead of
  `~/.openfang`.
- `OPENFANG_LISTEN` — the dashboard's bind address (default
  `127.0.0.1:4200`); override it if you need to run two instances at once.

No source changes were needed for this — `openfang` already reads both
variables. See `crates/openfang-kernel/src/config.rs` and
`crates/openfang-cli/src/launcher.rs`.
