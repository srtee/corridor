# Corridor

Terminal multiplexer with web-based message display.

## Installation

```bash
curl -sSL https://raw.githubusercontent.com/srtee/corridor/main/install.sh | sh
```

Or manually:
```bash
curl -L -o corridor https://github.com/srtee/corridor/releases/latest/download/corridor
chmod +x corridor
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

# Or run directly with Python
python3 corridor-terminal.py -s mysession
```

## Architecture

- `corridor` - Pre-built executable (or `corridor-terminal.py`) - Curses-based terminal multiplexer with pty and pyte for ANSI parsing. Main terminal area + 5-line bottom panel (1 separator + 4 message lines).
- `corridor-server.py` - HTTP server with web UI for sending messages.

## Environment Variables / CLI Args

- `-s, --session` - Session name (used by both server and terminal)
- `-u, --url` - Base URL for terminal to fetch messages from (default: http://localhost:8080)
