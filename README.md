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

## Display size and window icon

The UI starts at 1.4× zoom, which suits high-density displays. Change it under
"Einstellungen → Darstellungsgröße" or with Ctrl+Plus / Ctrl+Minus; the value is saved.

On X11, Windows and macOS the window icon (`assets/icon.png`) is set at runtime. Wayland
desktops take the icon from a `.desktop` file instead:

    install -Dm755 target/release/clamavui ~/.local/bin/clamavui
    install -Dm644 assets/clamavui.desktop ~/.local/share/applications/clamavui.desktop
    install -Dm644 assets/icon.png ~/.local/share/icons/hicolor/256x256/apps/clamavui.png
    install -Dm644 assets/icon.svg ~/.local/share/icons/hicolor/scalable/apps/clamavui.svg
    update-desktop-database ~/.local/share/applications
    kbuildsycoca6   # KDE Plasma only; GNOME picks the file up on its own

Then restart the app; the compositor matches the window's app id `clamavui` to the desktop file.

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

## Debian/Ubuntu: AppArmor

These distributions ship an AppArmor profile for `freshclam` that only permits
`/etc/clamav` and `/var/lib/clamav`, so the first update fails with
"Can't open/parse the config file". Allow the app's per-user directories once:

    sudo cp assets/apparmor/usr.bin.freshclam /etc/apparmor.d/local/usr.bin.freshclam
    sudo apparmor_parser -r /etc/apparmor.d/usr.bin.freshclam

`clamscan` has no profile and is not affected. The app shows the same hint when it
detects this error in the update log.

## License

MIT. ClamAV itself is only invoked as an external process.
