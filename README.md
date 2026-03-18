# Corridor

Terminal multiplexer with web-based message display.

## Installation

```bash
curl -sSL https://raw.githubusercontent.com/srtee/corridor/main/install.sh | sh
```

Or download manually from [releases](https://github.com/srtee/corridor/releases):
- Linux x86_64: `corridor-linux-x86_64`
- macOS x86_64: `corridor-macos-x86_64`
- macOS ARM: `corridor-macos-aarch64`

## Building from Source

```bash
git clone https://github.com/srtee/corridor.git
cd corridor
cargo build --release
# Binary at target/release/corridor
```

## Usage

**Server (send messages via web):**
```bash
# Default session
python3 corridor-server.py

# Custom session and port
SESSION=mysession python3 corridor-server.py -p 4000
# Open http://localhost:4000?session=mysession
```

**Terminal (displays shell + messages):**
```bash
# With local server
corridor -s mysession

# With remote server
corridor -u https://my.domain.com -s mysession
```

## Keyboard Shortcuts

- `F5` - Retry connection when offline
- `F6` - Copy web panel content to prompt

## Architecture

- `corridor` - Rust-based terminal emulator with ratatui for rendering. Main terminal area + 5-line bottom panel (1 separator + 4 message lines).
- `corridor-server.py` - HTTP server with web UI for sending messages.

## Environment Variables / CLI Args

- `-s, --session` - Session name (used by both server and terminal)
- `-u, --url` - Base URL for terminal to fetch messages from (default: http://localhost:8080)

## Legacy Python Client

The original Python implementation is available on the `legacy-python` branch.
