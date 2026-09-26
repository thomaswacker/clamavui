# ClamAV UI

Graphical front end for the ClamAV virus scanner (Linux, Windows, macOS), written in Rust with egui.

## Requirements

- ClamAV installed: `clamscan`, `freshclam` and `sigtool` must be in the PATH
  (or configured in the app's settings dialog).
- Linux build dependencies for egui/rfd: `libxkbcommon-dev`, `libgtk-3-dev`, `libwayland-dev`
  (Debian: `apt install libxkbcommon-dev libgtk-3-dev libwayland-dev`).

## Build and run

    cargo run --release

## Signatures

The app maintains its own signature database in the user's data directory
(Linux: `~/.local/share/clamavui/db`). The first update downloads about 300 MB.
The generated `freshclam.conf` lives in the config directory
(Linux: `~/.config/clamavui/freshclam.conf`) and can be edited by hand, for example to add `HTTPProxyServer`.

## License

MIT. ClamAV itself is only invoked as an external process.
