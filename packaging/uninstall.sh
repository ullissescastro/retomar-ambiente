#!/usr/bin/env bash
set -Eeuo pipefail

APP_ID="io.github.ullissescastro.RetomarAmbiente"
BIN="$HOME/.local/bin/retomar-ambiente"
UNINSTALLER="$HOME/.local/bin/retomar-ambiente-uninstall"

pkill -f "^$BIN --agent$" 2>/dev/null || true

rm -f \
    "$BIN" \
    "$HOME/.local/share/applications/$APP_ID.desktop" \
    "$HOME/.local/share/metainfo/$APP_ID.metainfo.xml" \
    "$HOME/.local/share/icons/hicolor/scalable/apps/$APP_ID.svg" \
    "$HOME/.config/autostart/$APP_ID.desktop" \
    "$HOME/.config/autostart/$APP_ID-agent.desktop"

command -v update-desktop-database >/dev/null 2>&1 \
    && update-desktop-database "$HOME/.local/share/applications" >/dev/null 2>&1 \
    || true

rm -f "$UNINSTALLER"

echo "Retomar Ambiente removido."
echo "Preferências e retratos de sessão foram preservados."
