#!/usr/bin/env bash
set -euo pipefail

APP_ID="io.github.ullissescastro.RetomarAmbiente"
BIN="$HOME/.local/bin/retomar-ambiente"

pkill -f "^$BIN --agent$" 2>/dev/null || true
rm -f \
  "$BIN" \
  "$HOME/.local/share/applications/$APP_ID.desktop" \
  "$HOME/.local/share/icons/hicolor/scalable/apps/$APP_ID.svg" \
  "$HOME/.config/autostart/$APP_ID.desktop" \
  "$HOME/.config/autostart/$APP_ID-agent.desktop"

echo "Retomar Ambiente removido. Preferências e retratos de sessão foram preservados."
