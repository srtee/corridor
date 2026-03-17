#!/bin/sh
# Install script for corridor-terminal
# Works with sh, bash, dash, and other POSIX shells

set -e

# Configuration
BIN_NAME="corridor-terminal"
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

# Detect OS and set executable name
case "$(uname -s)" in
    Linux*)     EXECUTABLE="${BIN_NAME}" ;;
    Darwin*)    EXECUTABLE="${BIN_NAME}-macos" ;;
    MINGW*|MSYS*|CYGWIN*) EXECUTABLE="${BIN_NAME}.exe" ;;
    *)          EXECUTABLE="${BIN_NAME}" ;;
esac

# Create install directory if it doesn't exist
echo "Installing to ${INSTALL_DIR}..."
mkdir -p "${INSTALL_DIR}"

# Download the executable
DOWNLOAD_URL="${REPO_URL}/${EXECUTABLE}"
echo "Downloading ${DOWNLOAD_URL}..."

if command -v curl >/dev/null 2>&1; then
    curl -fsSL "${DOWNLOAD_URL}" -o "${INSTALL_DIR}/${BIN_NAME}"
elif command -v wget >/dev/null 2>&1; then
    wget -q "${DOWNLOAD_URL}" -O "${INSTALL_DIR}/${BIN_NAME}"
else
    echo "Error: curl or wget is required to download the executable"
    exit 1
fi

# Make executable
chmod 755 "${INSTALL_DIR}/${BIN_NAME}"

# Check if install directory is in PATH
PATH_LINE="export PATH=\"\${HOME}/.local/bin:\${PATH}\""

if [ -f "${HOME}/.bashrc" ]; then
    if ! grep -q "${INSTALL_DIR}" "${HOME}/.bashrc" 2>/dev/null; then
        echo ""
        echo "Adding ${INSTALL_DIR} to PATH in ${HOME}/.bashrc..."
        echo "" >> "${HOME}/.bashrc"
        echo "# Added by corridor install script" >> "${HOME}/.bashrc"
        echo "${PATH_LINE}" >> "${HOME}/.bashrc"
        echo "Please run: source ${HOME}/.bashrc"
    fi
fi

# Also check .profile for other shells
if [ -f "${HOME}/.profile" ] && ! grep -q "${INSTALL_DIR}" "${HOME}/.profile" 2>/dev/null; then
    echo "" >> "${HOME}/.profile"
    echo "# Added by corridor install script" >> "${HOME}/.profile"
    echo "${PATH_LINE}" >> "${HOME}/.profile"
fi

echo "Installation complete!"
echo "Run 'corridor-terminal' to start, or restart your shell."
