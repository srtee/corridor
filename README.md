# Corridor

Terminal assistant display with web control.

## Usage

**Terminal (helper sees commands):**
```bash
python3 corridor.py mysession
```

**Web (you type commands):**
```bash
SESSION=mysession python3 server.py
# Open http://localhost:8080?session=mysession
```

## Recommended: tmux split

To keep coding while corridor runs, use tmux:

```bash
# Split terminal horizontally
tmux split-window -v

# Run corridor in the bottom pane
python3 corridor.py mysession

# Use the top pane for your shell
```

## JSON format (for direct file editing)

`/tmp/corridor-{session}.json`:
```json
{"message": "command here", "hint": "hint here"}
```
