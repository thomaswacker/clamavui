# ClamAV UI

Graphical front end for the ClamAV virus scanner (Linux, Windows, macOS), written in Rust with egui.

## Requirements

- ClamAV installed: `clamscan`, `freshclam` and `sigtool` must be in the PATH
  (or configured in the app's settings dialog).
- Linux build dependencies for egui/rfd: `libxkbcommon-dev`, `libgtk-3-dev`, `libwayland-dev`
  (Debian: `apt install libxkbcommon-dev libgtk-3-dev libwayland-dev`).
- Rust 1.95 or newer (`rustup update` if your toolchain is older).

## Downloads and CI

Every push to `main` builds and tests on Linux, Windows and macOS (Apple Silicon; the Intel build is cross-compiled there)
via GitHub Actions; the binaries are attached to the workflow run as artifacts. Pushing a tag
`v*` additionally publishes a GitHub release with all four archives.

## Installation

Prebuilt binaries are attached to every [release](https://github.com/thomaswacker/clamavui/releases).
They are not code-signed, so the operating system warns once on first start.

### macOS

1. Install ClamAV with Homebrew: `brew install clamav`. The app finds the tools in
   `/opt/homebrew/bin` (Apple Silicon) or `/usr/local/bin` (Intel) automatically.
2. Download `clamavui-macos-aarch64.tar.gz` (Apple Silicon) or `clamavui-macos-x86_64.tar.gz` (Intel)
   and unpack it: `tar -xzf clamavui-macos-*.tar.gz`.
3. Remove the quarantine flag once, then start the app:

       xattr -d com.apple.quarantine clamavui
       ./clamavui

   Alternatively right-click `clamavui` in Finder and choose "Open" to bypass Gatekeeper.
4. Optional: move `clamavui` to `/usr/local/bin` or `~/bin` to have it in your PATH.

### Windows

1. Install ClamAV from the official installer at <https://www.clamav.net/downloads>
   (default location `C:\Program Files\ClamAV`, which the app checks automatically).
   If you install elsewhere, enter the paths under "Einstellungen".
2. Download `clamavui-windows-x86_64.zip`, unpack it, and run `clamavui.exe`.
3. SmartScreen shows "Windows protected your PC" on first start: click "More info", then "Run anyway".

### Linux

1. Install ClamAV: `sudo apt install clamav clamav-freshclam` (Debian/Ubuntu) or the equivalent package.
2. Download `clamavui-linux-x86_64.tar.gz`, unpack it, and run `./clamavui`. On Debian/Ubuntu also
   see the AppArmor note below before the first signature update.

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

## macOS

Prerequisite: [Homebrew](https://brew.sh) and ClamAV (`brew install clamav`).

**Quick install (Apple Silicon and Intel):**

```bash
curl -fsSL https://raw.githubusercontent.com/thomaswacker/clamavui/main/install_clamavui_macos.sh | bash
```

Prefer to read the script first?

```bash
curl -fsSL -O https://raw.githubusercontent.com/thomaswacker/clamavui/main/install_clamavui_macos.sh
less install_clamavui_macos.sh
bash install_clamavui_macos.sh
```

The script downloads the latest release for your CPU, removes the quarantine
flag, wraps the binary in `ClamAV UI.app` (with icon) and installs it to
`/Applications` (or `~/Applications` if `/Applications` is not writable).
Run it again at any time to update.

On first start click "Aktualisieren" to download the signatures (about 300 MB).
ClamAV UI keeps its own signature database and `freshclam.conf`; the Homebrew
configuration is not used and does not need to be changed.

**Uninstall:** delete `ClamAV UI.app` from `/Applications`. Signature database
and settings are stored separately (see "Signatures" below).

<details>
<summary>Manual installation</summary>

(existing steps 2-4 here)

</details>

#### Troubleshooting (macOS)

- **"clamscan/freshclam/sigtool not found"**: apps started from Finder do not
  inherit your shell's PATH. Homebrew locations are detected automatically;
  other paths can be set under "Einstellungen".
- **Gatekeeper warning on first start**: right-click the app, choose "Öffnen",
  or allow it under System Settings → Privacy & Security.
- **Download fails**: the script needs a published release (tag `v*`).

## License

MIT. ClamAV itself is only invoked as an external process.
