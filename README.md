# Corridor

Terminal multiplexer with web-based message display.

## Installation

```bash
curl -sSL https://raw.githubusercontent.com/srtee/corridor/main/install.sh | sh
```

Or manually:
```bash
curl -L -o corridor-terminal https://github.com/srtee/corridor/releases/latest/download/corridor-terminal
chmod +x corridor-terminal
```

## Usage

**Terminal (displays shell + messages):**
```bash
# Pre-built executable (recommended)
SESSION=mysession ./corridor-terminal

# Or run directly with Python
SESSION=mysession python3 corridor-terminal.py

# With custom URL
URL=http://localhost:4000 SESSION=mysession ./corridor-terminal
```

**Server (send messages via web):**
```bash
# Default port 8080
SESSION=mysession python3 corridor-server.py

# Custom port
python3 corridor-server.py -p 4000
# Open http://localhost:4000?session=mysession
```

## Architecture

- `corridor-terminal` - Pre-built executable (or `corridor-terminal.py`) - Curses-based terminal multiplexer with pty and pyte for ANSI parsing. Main terminal area + 5-line bottom panel (1 separator + 4 message lines).
- `corridor-server.py` - HTTP server with web UI for sending messages.

## Environment Variables

- `SESSION` - Session name (used by both server and terminal)
- `URL` - Base URL for terminal to fetch messages from (default: http://localhost:8080)
