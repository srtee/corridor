# Corridor - Terminal Assistant Display

## Project Overview
- **Project name**: Corridor
- **Type**: Two-component system (ncurses terminal app + web front-end)
- **Core functionality**: Display commands/hints in a fixed 4-row panel at the bottom of the terminal, controlled remotely via web interface
- **Target users**: Someone helping others in a terminal - they type commands on web, helper sees them in terminal

## Architecture

### Sync Mechanism
- Shared JSON file in `/tmp/corridor-{session}.json`
- Web component writes to file
- Terminal app polls file every 500ms
- Terminal app is read-only (no writes needed)

### Components
1. **Terminal App** (`corridor.py`) - ncurses display at bottom 4 rows
2. **Web Server** (`server.py`) - lightweight HTTP server with web UI

---

## Component 1: Terminal App (corridor.py)

### UI/UX Specification

**Layout**
- Fixed 4 rows at bottom of terminal
- Full terminal width
- Non-blocking display (other work can continue above)

**Visual Design**
- Color scheme: Terminal green on dark background
  - Background: `#0d1117` (GitHub dark)
  - Primary text: `#58a6ff` (blue accent)
  - Secondary text: `#8b949e` (muted gray)
  - Highlight: `#238636` (green for session indicator)
  - Border: `#30363d`
- Border: Single-line box around the 4-row panel
- Top border shows session name + "CORRIDOR" title
- Content area: 2 rows for main content, 1 row for secondary hint

**Typography**
- Font: System monospace (no custom fonts needed)
- Font size: Terminal default

**Behavior**
- On launch: Prompt for session name (or accept as argument)
- Poll sync file every 500ms for updates
- Display "waiting for commands..." when empty
- Display up to 2 lines of content in main area
- Display 1 line of hint/secondary info below
- Clear display on `Ctrl+C` exit

### Functionality
- Accept session name as CLI argument or prompt
- Read from `/tmp/corridor-{session}.json`
- JSON format: `{"message": "string", "hint": "string"}`
- Handle missing file gracefully (show "waiting...")
- Graceful exit on `Ctrl+C`

---

## Component 2: Web Front-end (server.py)

### UI/UX Specification

**Layout**
- Minimal single-page interface
- Session name display at top
- Large textarea for message input
- Smaller input for hint/secondary line
- Send button + keyboard shortcut (Ctrl+Enter)

**Visual Design**
- Dark theme matching terminal app
- Background: `#0d1117`
- Text: `#c9d1d9`
- Accent button: `#238636` → `#2ea043` on hover
- Input fields: `#0d1117` bg with `#30363d` border
- Minimal spacing, centered container max-width 600px
- "Live" indicator showing connected session

**Typography**
- Font: `"JetBrains Mono", "Fira Code", monospace`
- Title: 24px bold
- Inputs: 16px
- Button: 14px uppercase

### Functionality
- HTTP server on port 8080 (configurable via PORT env)
- Serve static HTML + handle POST to update sync file
- Read session name from URL query: `?session=abc`
- POST endpoint `/api/update` with JSON `{"message": "...", "hint": "..."}`
- Write to `/tmp/corridor-{session}.json`
- Simple, no authentication

---

## Acceptance Criteria

1. Terminal app displays 4-row fixed panel at bottom
2. Terminal app responds to session name argument
3. Web front-end accessible at `http://localhost:8080?session=test`
4. Typing message in web and submitting appears in terminal within 1 second
5. Hint field works similarly
6. Both components can be killed gracefully
7. Works on terminals supporting 80+ columns
8. No external dependencies beyond Python standard lib + ncurses
