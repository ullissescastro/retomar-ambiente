#!/usr/bin/env bash
set -Eeuo pipefail

RELEASE_TAG="${1:?Informe a tag, por exemplo: v0.2.1}"
VERSION="${RELEASE_TAG#v}"
APP_ID="io.github.ullissescastro.RetomarAmbiente"
PACKAGE_NAME="retomar-ambiente-${VERSION}-linux-x86_64"
PACKAGE_ROOT="dist/${PACKAGE_NAME}"

[[ "$(uname -s)" == "Linux" ]] || {
    echo "Este empacotador deve ser executado no Linux." >&2
    exit 1
}

case "$(uname -m)" in
    x86_64|amd64) ;;
    *)
        echo "Este empacotador gera somente o pacote x86_64." >&2
        exit 1
        ;;
esac

[[ -x target/release/retomar-ambiente ]] || {
    echo "Binário release não encontrado." >&2
    exit 1
}

rm -rf dist
mkdir -p "$PACKAGE_ROOT/resources/icons"

strip --strip-unneeded target/release/retomar-ambiente

install -m0755 \
    target/release/retomar-ambiente \
    "$PACKAGE_ROOT/retomar-ambiente"

install -m0755 packaging/install.sh "$PACKAGE_ROOT/install.sh"
install -m0755 packaging/uninstall.sh "$PACKAGE_ROOT/uninstall.sh"

install -m0644 resources/app.desktop "$PACKAGE_ROOT/resources/app.desktop"
install -m0644 resources/autostart.desktop "$PACKAGE_ROOT/resources/autostart.desktop"
install -m0644 resources/agent-autostart.desktop "$PACKAGE_ROOT/resources/agent-autostart.desktop"
install -m0644 resources/app.metainfo.xml "$PACKAGE_ROOT/resources/app.metainfo.xml"
install -m0644 \
    resources/icons/hicolor/scalable/apps/icon.svg \
    "$PACKAGE_ROOT/resources/icons/${APP_ID}.svg"

install -m0644 LICENSE "$PACKAGE_ROOT/LICENSE"
install -m0644 README.md "$PACKAGE_ROOT/README.md"
install -m0644 CHANGELOG.md "$PACKAGE_ROOT/CHANGELOG.md"

ldd "$PACKAGE_ROOT/retomar-ambiente" > "$PACKAGE_ROOT/DEPENDENCIES.txt"

cat > "$PACKAGE_ROOT/VERSION" <<EOF
${VERSION}
EOF

tar -C dist -czf "dist/${PACKAGE_NAME}.tar.gz" "$PACKAGE_NAME"

(
    cd dist
    sha256sum "${PACKAGE_NAME}.tar.gz" > "${PACKAGE_NAME}.tar.gz.sha256"
)

if [[ -n "${GITHUB_ENV:-}" ]]; then
    echo "PACKAGE_NAME=$PACKAGE_NAME" >> "$GITHUB_ENV"
fi

ls -lh "dist/${PACKAGE_NAME}.tar.gz" "dist/${PACKAGE_NAME}.tar.gz.sha256"
cat "dist/${PACKAGE_NAME}.tar.gz.sha256"
