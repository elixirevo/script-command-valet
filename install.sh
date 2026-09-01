#!/usr/bin/env bash
set -euo pipefail

VERSION="${SCV_VERSION:-latest}"
INSTALL_DIR="${SCV_INSTALL_DIR:-$HOME/.local/bin}"
RELEASE_BASE_URL="${SCV_RELEASE_BASE_URL:-https://github.com/elixirevo/script-command-valet/releases}"
BINARY_PATH=""
MODIFY_PATH=true

usage() {
    printf '%s\n' 'Usage: install.sh [--version <version>] [--install-dir <path>] [--binary <path>] [--no-modify-path]'
}

while [[ $# -gt 0 ]]; do
    case "$1" in
        --version)
            [[ $# -ge 2 ]] || { printf 'install.sh: --version requires a value\n' >&2; exit 2; }
            VERSION="$2"
            shift 2
            ;;
        --install-dir)
            [[ $# -ge 2 ]] || { printf 'install.sh: --install-dir requires a value\n' >&2; exit 2; }
            INSTALL_DIR="$2"
            shift 2
            ;;
        --binary)
            [[ $# -ge 2 ]] || { printf 'install.sh: --binary requires a value\n' >&2; exit 2; }
            BINARY_PATH="$2"
            shift 2
            ;;
        --no-modify-path)
            MODIFY_PATH=false
            shift
            ;;
        -h|--help)
            usage
            exit 0
            ;;
        *)
            printf 'install.sh: unknown option: %s\n' "$1" >&2
            usage >&2
            exit 2
            ;;
    esac
done

case "$(uname -s)" in
    Darwin) OS=apple-darwin ;;
    Linux) OS=unknown-linux-gnu ;;
    *) printf 'install.sh: unsupported operating system: %s\n' "$(uname -s)" >&2; exit 1 ;;
esac

case "$(uname -m)" in
    arm64|aarch64) ARCH=aarch64 ;;
    x86_64|amd64) ARCH=x86_64 ;;
    *) printf 'install.sh: unsupported architecture: %s\n' "$(uname -m)" >&2; exit 1 ;;
esac

mkdir -p "$INSTALL_DIR"

if [[ -n "$BINARY_PATH" ]]; then
    [[ -f "$BINARY_PATH" ]] || { printf 'install.sh: binary not found: %s\n' "$BINARY_PATH" >&2; exit 1; }
    install -m 755 "$BINARY_PATH" "$INSTALL_DIR/scv"
else
    command -v curl >/dev/null 2>&1 || { printf 'install.sh: curl is required\n' >&2; exit 1; }
    TEMP_DIR="$(mktemp -d "${TMPDIR:-/tmp}/scv-install.XXXXXXXX")"
    cleanup() {
        [[ -n "${TEMP_DIR:-}" && -d "$TEMP_DIR" ]] && rm -rf "$TEMP_DIR"
    }
    trap cleanup EXIT

    ARTIFACT="scv-${ARCH}-${OS}.tar.gz"
    if [[ "$VERSION" == latest ]]; then
        DOWNLOAD_URL="$RELEASE_BASE_URL/latest/download/$ARTIFACT"
        CHECKSUM_URL="$RELEASE_BASE_URL/latest/download/SHA256SUMS"
    else
        DOWNLOAD_URL="$RELEASE_BASE_URL/download/$VERSION/$ARTIFACT"
        CHECKSUM_URL="$RELEASE_BASE_URL/download/$VERSION/SHA256SUMS"
    fi
    curl --fail --location --proto '=https' --tlsv1.2 "$DOWNLOAD_URL" --output "$TEMP_DIR/$ARTIFACT"
    curl --fail --location --proto '=https' --tlsv1.2 "$CHECKSUM_URL" --output "$TEMP_DIR/SHA256SUMS"
    EXPECTED="$(awk -v artifact="$ARTIFACT" '$2 == artifact { print $1 }' "$TEMP_DIR/SHA256SUMS")"
    [[ -n "$EXPECTED" ]] || { printf 'install.sh: checksum missing for %s\n' "$ARTIFACT" >&2; exit 1; }
    if command -v shasum >/dev/null 2>&1; then
        ACTUAL="$(shasum -a 256 "$TEMP_DIR/$ARTIFACT" | awk '{print $1}')"
    else
        ACTUAL="$(sha256sum "$TEMP_DIR/$ARTIFACT" | awk '{print $1}')"
    fi
    [[ "$ACTUAL" == "$EXPECTED" ]] || { printf 'install.sh: checksum verification failed\n' >&2; exit 1; }
    tar -xzf "$TEMP_DIR/$ARTIFACT" -C "$TEMP_DIR"
    install -m 755 "$TEMP_DIR/scv" "$INSTALL_DIR/scv"
fi

path_contains() {
    case ":$PATH:" in
        *":$INSTALL_DIR:"*) return 0 ;;
        *) return 1 ;;
    esac
}

append_managed_block() {
    local profile="$1"
    local line="$2"
    local start='# >>> scv >>>'
    if [[ -f "$profile" ]] && grep -Fq "$start" "$profile"; then
        return
    fi
    mkdir -p "$(dirname "$profile")"
    {
        printf '\n%s\n' "$start"
        printf '%s\n' "$line"
        printf '%s\n' '# <<< scv <<<'
    } >>"$profile"
    printf 'Updated PATH configuration: %s\n' "$profile"
}

if [[ "$MODIFY_PATH" == true && "${SCV_NO_MODIFY_PATH:-0}" != 1 ]] && ! path_contains; then
    case "${SCV_SHELL:-$(basename "${SHELL:-sh}")}" in
        zsh) append_managed_block "${SCV_SHELL_PROFILE:-$HOME/.zshenv}" "export PATH=\"$INSTALL_DIR:\$PATH\"" ;;
        bash) append_managed_block "${SCV_SHELL_PROFILE:-$HOME/.bashrc}" "export PATH=\"$INSTALL_DIR:\$PATH\"" ;;
        fish) append_managed_block "${SCV_SHELL_PROFILE:-$HOME/.config/fish/conf.d/scv.fish}" "fish_add_path \"$INSTALL_DIR\"" ;;
        *)
            printf 'Add this directory to PATH: %s\n' "$INSTALL_DIR"
            ;;
    esac
fi

printf 'Installed SCV: %s\n' "$INSTALL_DIR/scv"
"$INSTALL_DIR/scv" --version
