#!/bin/bash
set -e

APP_NAME="mzprotokoll"
BIN_DIR="$HOME/.local/bin"
DESKTOP_DIR="$HOME/.local/share/applications"
ICON_DIR="$HOME/.local/share/icons/hicolor/256x256/apps"
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"

echo "Installing $APP_NAME..."

# Binary bauen (Release)
echo "Building release binary..."
cargo build --release --manifest-path "$SCRIPT_DIR/Cargo.toml"

# Ordner erstellen
mkdir -p "$BIN_DIR" "$DESKTOP_DIR" "$ICON_DIR"

# Binary installieren
cp "$SCRIPT_DIR/target/release/$APP_NAME" "$BIN_DIR/$APP_NAME"
chmod +x "$BIN_DIR/$APP_NAME"

# Icon installieren
cp "$SCRIPT_DIR/assets/icon.png" "$ICON_DIR/$APP_NAME.png"

# Desktop-Datei installieren (mit korrektem Pfad)
cat > "$DESKTOP_DIR/$APP_NAME.desktop" << EOF
[Desktop Entry]
Name=MZProtokoll
Comment=MZProtokoll
Exec=$BIN_DIR/$APP_NAME
Icon=$APP_NAME
Type=Application
Categories=Office;
Terminal=false
EOF

echo "Done! $APP_NAME installed to $BIN_DIR/$APP_NAME"
echo "Desktop entry created. App should appear in your launcher."
