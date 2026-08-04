#!/usr/bin/env bash
set -Eeuo pipefail

ROOT="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
APP_ID="io.github.ullissescastro.RetomarAmbiente"
BIN_DIR="$HOME/.local/bin"
APP_DIR="$HOME/.local/share/applications"
METAINFO_DIR="$HOME/.local/share/metainfo"
ICON_DIR="$HOME/.local/share/icons/hicolor/scalable/apps"
AUTOSTART_DIR="$HOME/.config/autostart"
STATE_DIR="${XDG_STATE_HOME:-$HOME/.local/state}/retomar-ambiente"
PROMPT_AUTOSTART="$AUTOSTART_DIR/$APP_ID.desktop"
AGENT_AUTOSTART="$AUTOSTART_DIR/$APP_ID-agent.desktop"

[[ "$(uname -s)" == "Linux" ]] || {
    echo "Erro: este pacote é destinado ao Linux." >&2
    exit 1
}

case "$(uname -m)" in
    x86_64|amd64) ;;
    *)
        echo "Erro: este pacote requer uma máquina x86_64." >&2
        exit 1
        ;;
esac

[[ -x "$ROOT/retomar-ambiente" ]] || {
    echo "Erro: binário do Retomar Ambiente não encontrado." >&2
    exit 1
}

mkdir -p \
    "$BIN_DIR" \
    "$APP_DIR" \
    "$METAINFO_DIR" \
    "$ICON_DIR" \
    "$AUTOSTART_DIR" \
    "$STATE_DIR"

install -m0755 "$ROOT/retomar-ambiente" "$BIN_DIR/retomar-ambiente"
install -m0644 "$ROOT/resources/icons/$APP_ID.svg" "$ICON_DIR/$APP_ID.svg"
install -m0644 "$ROOT/resources/app.metainfo.xml" "$METAINFO_DIR/$APP_ID.metainfo.xml"

sed "s|^Exec=retomar-ambiente|Exec=$BIN_DIR/retomar-ambiente|" \
    "$ROOT/resources/app.desktop" > "$APP_DIR/$APP_ID.desktop"
chmod 0644 "$APP_DIR/$APP_ID.desktop"

sed "s|^Exec=retomar-ambiente|Exec=$BIN_DIR/retomar-ambiente|" \
    "$ROOT/resources/agent-autostart.desktop" > "$AGENT_AUTOSTART"
chmod 0644 "$AGENT_AUTOSTART"

ASK_FILE="$HOME/.config/cosmic/$APP_ID/v1/ask_on_login"
if [[ ! -f "$ASK_FILE" ]] || ! grep -qx 'false' "$ASK_FILE"; then
    sed "s|^Exec=retomar-ambiente|Exec=$BIN_DIR/retomar-ambiente|" \
        "$ROOT/resources/autostart.desktop" > "$PROMPT_AUTOSTART"
    chmod 0644 "$PROMPT_AUTOSTART"
else
    rm -f "$PROMPT_AUTOSTART"
fi

install -m0755 "$ROOT/uninstall.sh" "$BIN_DIR/retomar-ambiente-uninstall"

command -v update-desktop-database >/dev/null 2>&1 \
    && update-desktop-database "$APP_DIR" >/dev/null 2>&1 \
    || true

command -v gtk-update-icon-cache >/dev/null 2>&1 \
    && gtk-update-icon-cache -f -t "$HOME/.local/share/icons/hicolor" >/dev/null 2>&1 \
    || true

pkill -f "^$BIN_DIR/retomar-ambiente --agent$" 2>/dev/null || true
nohup "$BIN_DIR/retomar-ambiente" --agent \
    >>"$STATE_DIR/agent.log" 2>&1 </dev/null &

echo
echo "Retomar Ambiente instalado para o usuário atual."
echo "O agente já está registrando os aplicativos elegíveis."
echo
echo "Para remover:"
echo "  retomar-ambiente-uninstall"
