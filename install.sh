#!/usr/bin/env bash
#
# Installer for Rustrest (https://github.com/SojebSikder/rustrest)
#
# Downloads the latest (or a pinned) release archive from GitHub, verifies its
# sha256 checksum, and installs the `rustrest` binary along with desktop shortcuts/icons.
#
# Usage:
#   curl --proto '=https' --tlsv1.2 -LsSf https://raw.githubusercontent.com/SojebSikder/rustrest/main/install.sh | sh
#
# Environment variables:
#   VERSION      Release tag to install, e.g. "v0.1.2" (default: latest release)
#   INSTALL_DIR  Directory to install the binary into (default: "$HOME/.local/bin")

set -eu

REPO="SojebSikder/rustrest"
BINARY_NAME="rustrest"
APP_NAME="Rustrest"
INSTALL_DIR="${INSTALL_DIR:-$HOME/.local/bin}"
VERSION="${VERSION:-}"
tmp_dir=""

info() { printf '\033[1;34m==>\033[0m %s\n' "$1"; }
error() { printf '\033[1;31merror:\033[0m %s\n' "$1" >&2; exit 1; }

need_cmd() {
  command -v "$1" >/dev/null 2>&1 || error "required command '$1' not found on PATH"
}

detect_target() {
  local os arch
  os="$(uname -s)"
  arch="$(uname -m)"

  case "$os" in
    Linux) os="unknown-linux-gnu" ;;
    Darwin) os="apple-darwin" ;;
    *) error "unsupported OS: $os (Windows users should download the .zip from the Releases page instead)" ;;
  esac

  case "$arch" in
    x86_64|amd64) arch="x86_64" ;;
    arm64|aarch64) arch="aarch64" ;;
    *) error "unsupported architecture: $arch" ;;
  esac

  printf '%s-%s' "$arch" "$os"
}

install_desktop_integration() {
  local raw_repo_url="https://raw.githubusercontent.com/${REPO}/main"
  local os_type
  os_type="$(uname -s)"

  if [ "$os_type" = "Linux" ]; then
    info "Setting up Linux desktop entry and icon..."

    local icon_dir="$HOME/.local/share/icons/hicolor/256x256/apps"
    local desktop_dir="$HOME/.local/share/applications"

    mkdir -p "$icon_dir" "$desktop_dir"

    # Download high-res application icon
    curl --proto '=https' --tlsv1.2 -LsSf -o "${icon_dir}/rustrest.png" \
      "${raw_repo_url}/assets/images/logo-transparent.png" 2>/dev/null || true

    # Create .desktop entry for application launchers (GNOME, KDE, etc.)
    cat <<EOF > "${desktop_dir}/rustrest.desktop"
[Desktop Entry]
Type=Application
Name=${APP_NAME}
Comment=API Testing Platform
Exec=${INSTALL_DIR}/${BINARY_NAME}
Icon=rustrest
Terminal=false
Categories=Development;
EOF

    # Update desktop icon cache if tools exist
    if command -v gtk-update-icon-cache >/dev/null 2>&1; then
      gtk-update-icon-cache -f -t "$HOME/.local/share/icons/hicolor" >/dev/null 2>&1 || true
    fi

    info "Desktop launcher installed to ${desktop_dir}/rustrest.desktop"
  fi
}

main() {
  need_cmd curl
  need_cmd tar
  need_cmd mktemp

  local target archive_name checksum_cmd archive_path checksum_path expected_sum actual_sum extracted_bin

  target="$(detect_target)"

  if [ -z "$VERSION" ]; then
    info "Looking up latest release..."
    VERSION="$(curl --proto '=https' --tlsv1.2 -sS -o /dev/null -w '%{redirect_url}' \
      "https://github.com/${REPO}/releases/latest" | sed -E 's#.*/tag/##')"
    [ -n "$VERSION" ] || error "could not determine the latest release version"
  fi

  archive_name="${BINARY_NAME}-${target}.tar.xz"
  local base_url="https://github.com/${REPO}/releases/download/${VERSION}"

  tmp_dir="$(mktemp -d)"
  trap 'rm -rf "$tmp_dir"' EXIT

  archive_path="${tmp_dir}/${archive_name}"
  checksum_path="${archive_path}.sha256"

  info "Downloading ${archive_name} (${VERSION})..."
  curl --proto '=https' --tlsv1.2 -LsSf -o "$archive_path" "${base_url}/${archive_name}" \
    || error "failed to download ${base_url}/${archive_name}"
  curl --proto '=https' --tlsv1.2 -LsSf -o "$checksum_path" "${base_url}/${archive_name}.sha256" \
    || error "failed to download checksum file"

  info "Verifying checksum..."
  expected_sum="$(cut -d ' ' -f1 "$checksum_path")"
  if command -v sha256sum >/dev/null 2>&1; then
    actual_sum="$(sha256sum "$archive_path" | cut -d ' ' -f1)"
  elif command -v shasum >/dev/null 2>&1; then
    actual_sum="$(shasum -a 256 "$archive_path" | cut -d ' ' -f1)"
  else
    error "need either 'sha256sum' or 'shasum' to verify the download"
  fi
  [ "$expected_sum" = "$actual_sum" ] || error "checksum mismatch for ${archive_name}"

  info "Extracting..."
  tar -xf "$archive_path" -C "$tmp_dir"

  extracted_bin="$(find "$tmp_dir" -type f -name "$BINARY_NAME" | head -n1)"
  [ -n "$extracted_bin" ] || error "could not find '${BINARY_NAME}' binary inside the downloaded archive"

  mkdir -p "$INSTALL_DIR"
  install -m 755 "$extracted_bin" "${INSTALL_DIR}/${BINARY_NAME}"

  info "Installed ${BINARY_NAME} ${VERSION} to ${INSTALL_DIR}/${BINARY_NAME}"

  # Add desktop environment integration (icons and launchers)
  install_desktop_integration

  case ":$PATH:" in
    *":$INSTALL_DIR:"*) ;;
    *) info "Note: ${INSTALL_DIR} is not on your PATH. Add this to your shell profile:
       export PATH=\"${INSTALL_DIR}:\$PATH\"" ;;
  esac
}

main "$@"
