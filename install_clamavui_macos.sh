#!/usr/bin/env bash
#
# Installiert ClamAV UI (https://github.com/thomaswacker/clamavui) auf macOS.
#
# - lädt das aktuelle Release passend zur CPU-Architektur herunter
# - entfernt das Quarantäne-Flag (xattr -d com.apple.quarantine)
# - baut ein App-Bundle "ClamAV UI.app" (mit Icon) und installiert es
#   nach /Applications (bzw. ~/Applications, falls kein Schreibzugriff)
#
# Aufruf:  bash install-clamavui.sh
#
set -euo pipefail

REPO="thomaswacker/clamavui"
APP_NAME="ClamAV UI"
BIN_NAME="clamavui"
BUNDLE_ID="com.github.thomaswacker.clamavui"

info() { printf '\033[1;34m==>\033[0m %s\n' "$*"; }
warn() { printf '\033[1;33m!!\033[0m  %s\n' "$*" >&2; }
die()  { printf '\033[1;31mFehler:\033[0m %s\n' "$*" >&2; exit 1; }

# --- Voraussetzungen ---------------------------------------------------------
[[ "$(uname -s)" == "Darwin" ]] || die "Dieses Script läuft nur auf macOS."

case "$(uname -m)" in
  arm64)  ARCH="aarch64" ;;
  x86_64) ARCH="x86_64"  ;;
  *)      die "Unbekannte Architektur: $(uname -m)" ;;
esac

export PATH="/opt/homebrew/bin:/usr/local/bin:$PATH"
for tool in clamscan freshclam sigtool; do
  command -v "$tool" >/dev/null 2>&1 \
    || die "'$tool' nicht gefunden. Bitte zuerst installieren: brew install clamav"
done

# --- Zielordner bestimmen ----------------------------------------------------
if [[ -w "/Applications" ]]; then
  DEST_DIR="/Applications"
else
  DEST_DIR="$HOME/Applications"
  mkdir -p "$DEST_DIR"
  warn "Kein Schreibzugriff auf /Applications – installiere nach $DEST_DIR"
fi
APP_PATH="$DEST_DIR/$APP_NAME.app"

# --- Download ----------------------------------------------------------------
TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT

ASSET="clamavui-macos-${ARCH}.tar.gz"
URL="https://github.com/${REPO}/releases/latest/download/${ASSET}"

info "Lade $ASSET herunter ..."
curl -fL --progress-bar -o "$TMP/$ASSET" "$URL" \
  || die "Download fehlgeschlagen: $URL (existiert bereits ein Release?)"

info "Entpacke ..."
mkdir "$TMP/extract"
tar -xzf "$TMP/$ASSET" -C "$TMP/extract"

BIN_SRC="$(find "$TMP/extract" -type f -name "$BIN_NAME" | head -n 1)"
[[ -n "$BIN_SRC" ]] || die "Binary '$BIN_NAME' im Archiv nicht gefunden."

# Quarantäne-Flag entfernen und ausführbar machen
xattr -d com.apple.quarantine "$BIN_SRC" 2>/dev/null || true
chmod +x "$BIN_SRC"

# --- App-Bundle bauen --------------------------------------------------------
info "Erstelle App-Bundle ..."
BUNDLE="$TMP/$APP_NAME.app"
mkdir -p "$BUNDLE/Contents/MacOS" "$BUNDLE/Contents/Resources"
cp "$BIN_SRC" "$BUNDLE/Contents/MacOS/$BIN_NAME"

# Icon (optional): assets/icon.png -> icns
ICON_KEY=""
if curl -fsSL -o "$TMP/icon.png" \
     "https://raw.githubusercontent.com/${REPO}/main/assets/icon.png"; then
  ICONSET="$TMP/AppIcon.iconset"
  mkdir "$ICONSET"
  ok=1
  for size in 16 32 128 256 512; do
    sips -z "$size" "$size" "$TMP/icon.png" \
         --out "$ICONSET/icon_${size}x${size}.png" >/dev/null 2>&1 || ok=0
    sips -z "$((size * 2))" "$((size * 2))" "$TMP/icon.png" \
         --out "$ICONSET/icon_${size}x${size}@2x.png" >/dev/null 2>&1 || ok=0
  done
  if [[ $ok -eq 1 ]] && iconutil -c icns "$ICONSET" \
       -o "$BUNDLE/Contents/Resources/AppIcon.icns" 2>/dev/null; then
    ICON_KEY="<key>CFBundleIconFile</key><string>AppIcon</string>"
  else
    warn "Icon konnte nicht erzeugt werden – fahre ohne Icon fort."
  fi
else
  warn "Icon konnte nicht geladen werden – fahre ohne Icon fort."
fi

cat > "$BUNDLE/Contents/Info.plist" <<EOF
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleName</key><string>${APP_NAME}</string>
  <key>CFBundleDisplayName</key><string>${APP_NAME}</string>
  <key>CFBundleIdentifier</key><string>${BUNDLE_ID}</string>
  <key>CFBundleExecutable</key><string>${BIN_NAME}</string>
  <key>CFBundlePackageType</key><string>APPL</string>
  <key>CFBundleVersion</key><string>1</string>
  <key>CFBundleShortVersionString</key><string>1</string>
  <key>LSApplicationCategoryType</key><string>public.app-category.utilities</string>
  <key>NSHighResolutionCapable</key><true/>
  ${ICON_KEY}
</dict>
</plist>
EOF

# Ad-hoc-Signatur (nötig bzw. hilfreich für unsignierte Binaries auf Apple Silicon)
codesign --force --deep --sign - "$BUNDLE" >/dev/null 2>&1 \
  || warn "Ad-hoc-Signierung fehlgeschlagen (nicht kritisch)."

# --- Installieren ------------------------------------------------------------
if pgrep -x "$BIN_NAME" >/dev/null 2>&1; then
  warn "$APP_NAME läuft gerade – bitte nach der Installation neu starten."
fi

info "Installiere nach $APP_PATH ..."
rm -rf "$APP_PATH"
cp -R "$BUNDLE" "$APP_PATH"

# Quarantäne-Flag auch am installierten Bundle sicher entfernen
xattr -dr com.apple.quarantine "$APP_PATH" 2>/dev/null || true

# Launchpad/Spotlight über die neue App informieren
touch "$APP_PATH"
/System/Library/Frameworks/CoreServices.framework/Frameworks/LaunchServices.framework/Support/lsregister \
  -f "$APP_PATH" >/dev/null 2>&1 || true

info "Fertig. $APP_NAME ist jetzt unter Programme verfügbar."
echo "   Start:  open \"$APP_PATH\""
echo "   Beim ersten Start unter 'Aktualisieren' die Signaturen laden (ca. 300 MB)."