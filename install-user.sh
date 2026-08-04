#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
cd "$ROOT"

command -v just >/dev/null 2>&1 || {
    echo "Erro: o comando 'just' não está instalado." >&2
    exit 1
}

just build-release

BIN_DIR="$HOME/.local/bin"
APP_DIR="$HOME/.local/share/applications"
ICON_DIR="$HOME/.local/share/icons/hicolor/scalable/apps"
AUTOSTART_DIR="$HOME/.config/autostart"
STATE_DIR="${XDG_STATE_HOME:-$HOME/.local/state}/retomar-ambiente"
APP_ID="io.github.ullissescastro.RetomarAmbiente"
PROMPT_AUTOSTART="$AUTOSTART_DIR/$APP_ID.desktop"
AGENT_AUTOSTART="$AUTOSTART_DIR/$APP_ID-agent.desktop"

install -Dm0755 target/release/retomar-ambiente "$BIN_DIR/retomar-ambiente"
install -Dm0644 resources/icons/hicolor/scalable/apps/icon.svg "$ICON_DIR/$APP_ID.svg"
mkdir -p "$APP_DIR" "$AUTOSTART_DIR" "$STATE_DIR"

sed "s|^Exec=retomar-ambiente|Exec=$BIN_DIR/retomar-ambiente|" \
    resources/app.desktop > "$APP_DIR/$APP_ID.desktop"
chmod 0644 "$APP_DIR/$APP_ID.desktop"

sed "s|^Exec=retomar-ambiente|Exec=$BIN_DIR/retomar-ambiente|" \
    resources/agent-autostart.desktop > "$AGENT_AUTOSTART"
chmod 0644 "$AGENT_AUTOSTART"

ASK_FILE="$HOME/.config/cosmic/$APP_ID/v1/ask_on_login"
if [[ ! -f "$ASK_FILE" ]] || ! grep -qx 'false' "$ASK_FILE"; then
    sed "s|^Exec=retomar-ambiente|Exec=$BIN_DIR/retomar-ambiente|" \
        resources/autostart.desktop > "$PROMPT_AUTOSTART"
    chmod 0644 "$PROMPT_AUTOSTART"
else
    rm -f "$PROMPT_AUTOSTART"
fi

command -v update-desktop-database >/dev/null 2>&1 \
    && update-desktop-database "$APP_DIR" >/dev/null 2>&1 || true
command -v gtk-update-icon-cache >/dev/null 2>&1 \
    && gtk-update-icon-cache -f -t "$HOME/.local/share/icons/hicolor" >/dev/null 2>&1 || true

# Começa a registrar a sessão atual sem exigir um logout prévio.
pkill -f "^$BIN_DIR/retomar-ambiente --agent$" 2>/dev/null || true
nohup "$BIN_DIR/retomar-ambiente" --agent \
    >>"$STATE_DIR/agent.log" 2>&1 </dev/null &

printf '\nRetomar Ambiente 0.2.1 instalado para o usuário atual.\n'
printf 'O agente já está registrando os aplicativos elegíveis desta sessão.\n'
printf 'No próximo login, somente os que permanecerem abertos ao sair serão oferecidos.\n'
