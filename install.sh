#!/bin/sh
# Install script for corridor
# Works with sh, bash, dash, and other POSIX shells

set -e

# Configuration
BIN_NAME="corridor"
INSTALL_DIR="${HOME}/.local/bin"
REPO_URL="${REPO_URL:-https://github.com/srtee/corridor/releases/latest/download}"

usage() {
    echo "Usage: $0 [--repo-url URL]"
    echo "  --repo-url URL   Override the repository URL for downloads"
    exit 1
}

# Parse arguments
while [ $# -gt 0 ]; do
    case "$1" in
        --repo-url)
            REPO_URL="$2"
            shift 2
            ;;
        --help|-h)
            usage
            ;;
        *)
            echo "Unknown option: $1"
            usage
            ;;
    esac
done

# Detect OS and architecture
OS="$(uname -s)"
ARCH="$(uname -m)"

case "${OS}" in
    Linux*)
        case "${ARCH}" in
            x86_64|amd64) EXECUTABLE="corridor-linux-x86_64" ;;
            *)            echo "Unsupported architecture: ${ARCH}"; exit 1 ;;
        esac
        ;;
    Darwin*)
        case "${ARCH}" in
            x86_64|amd64) EXECUTABLE="corridor-macos-x86_64" ;;
            arm64|aarch64) EXECUTABLE="corridor-macos-aarch64" ;;
            *)            echo "Unsupported architecture: ${ARCH}"; exit 1 ;;
        esac
        ;;
    MINGW*|MSYS*|CYGWIN*)
        EXECUTABLE="corridor-windows-x86_64.exe"
        ;;
    *)
        echo "Unsupported OS: ${OS}"
        exit 1
        ;;
esac

# Create install directory if it doesn't exist
echo "Installing to ${INSTALL_DIR}..."
mkdir -p "${INSTALL_DIR}"

# Download the executable
DOWNLOAD_URL="${REPO_URL}/${EXECUTABLE}"
echo "Downloading ${DOWNLOAD_URL}..."

if command -v curl >/dev/null 2>&1; then
    curl -#SL "${DOWNLOAD_URL}" -o "${INSTALL_DIR}/${BIN_NAME}"
elif command -v wget >/dev/null 2>&1; then
    wget --show-progress "${DOWNLOAD_URL}" -O "${INSTALL_DIR}/${BIN_NAME}"
else
    echo "Error: curl or wget is required to download the executable"
    exit 1
fi

# Make executable
chmod 755 "${INSTALL_DIR}/${BIN_NAME}"

# Check if install directory is in PATH and add if needed
PATH_LINE="export PATH=\"\${HOME}/.local/bin:\${PATH}\""
BASHRC_ADDED=0
PROFILE_ADDED=0

if [ -f "${HOME}/.bashrc" ]; then
    if ! grep -q "${INSTALL_DIR}" "${HOME}/.bashrc" 2>/dev/null; then
        echo "" >> "${HOME}/.bashrc"
        echo "# Added by corridor install script" >> "${HOME}/.bashrc"
        echo "${PATH_LINE}" >> "${HOME}/.bashrc"
        BASHRC_ADDED=1
    fi
fi

if [ -f "${HOME}/.profile" ] && ! grep -q "${INSTALL_DIR}" "${HOME}/.profile" 2>/dev/null; then
    echo "" >> "${HOME}/.profile"
    echo "# Added by corridor install script" >> "${HOME}/.profile"
    echo "${PATH_LINE}" >> "${HOME}/.profile"
    PROFILE_ADDED=1
fi

echo "Installation complete!"
echo "Run 'corridor' to start."

if [ "${BASHRC_ADDED}" -eq 1 ]; then
    echo ""
    echo "Please run 'source ${HOME}/.bashrc' (without quotes), or restart your shell, before your first run."
elif [ "${PROFILE_ADDED}" -eq 1 ]; then
    echo ""
    echo "Please run 'source ${HOME}/.profile' (without quotes), or restart your shell, before your first run."
fi
