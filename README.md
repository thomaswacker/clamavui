# ClamAV UI

Graphical front end for the ClamAV virus scanner (Linux, Windows, macOS), written in Rust with egui.

## Requirements

- ClamAV installed: `clamscan`, `freshclam` and `sigtool` must be in the PATH
  (or configured in the app's settings dialog).
- Linux build dependencies for egui/rfd: `libxkbcommon-dev`, `libgtk-3-dev`, `libwayland-dev`
  (Debian: `apt install libxkbcommon-dev libgtk-3-dev libwayland-dev`).
- Rust 1.95 or newer (`rustup update` if your toolchain is older).

## Build and run

    cargo run --release

## Signatures

The app maintains its own signature database in the user's data directory
(Linux: `~/.local/share/clamavui/db`). The first update downloads about 300 MB.
The generated `freshclam.conf` lives in the config directory
(Linux: `~/.config/clamavui/freshclam.conf`) and can be edited by hand, for example to add `HTTPProxyServer`.

## Usage

1. On first start click "Aktualisieren" to download the signatures.
2. Choose files or folders (or drag them onto the window) and click "Scan starten".
3. Per finding: delete (with confirmation), move to trash, rename (suggested suffix `.infected`), ignore.

Log output: `RUST_LOG=debug cargo run`.

## Tests

    cargo test                                            # unit tests, no ClamAV needed
    cargo test --test clamscan_integration -- --ignored   # needs clamscan and a signature DB

## License

MIT. ClamAV itself is only invoked as an external process.
