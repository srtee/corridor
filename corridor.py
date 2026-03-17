#!/usr/bin/env python3
import time
import json
import os
import sys
import signal
import fcntl
import termios
import struct

SESSION_FILE = "/tmp/corridor-{session}.json"

RESET = "\033[0m"
BOLD = "\033[1m"
DIM = "\033[2m"
CYAN = "\033[36m"
GREEN = "\033[32m"
GRAY = "\033[90m"
BG_DARK = "\033[48;5;235m"
BORDER = "\033[38;5;240m"


def get_term_size():
    try:
        tty = os.open("/dev/tty", os.O_RDWR)
        try:
            dims = struct.unpack("hh", fcntl.ioctl(tty, termios.TIOCGWINSZ, b"1234"))
            return dims[1], dims[0]
        finally:
            os.close(tty)
    except:
        return 80, 24


def clear_line():
    sys.stdout.write("\033[2K")
    sys.stdout.flush()


def position(row, col):
    sys.stdout.write(f"\033[{row};{col}H")
    sys.stdout.flush()


def scroll_up(lines):
    sys.stdout.write(f"\033[{lines}S")
    sys.stdout.flush()


class CorridorDisplay:
    def __init__(self, session_name):
        self.session_name = session_name
        self.session_file = SESSION_FILE.format(session=session_name)
        self.running = True
        self.width = 80

    def get_content(self):
        try:
            if os.path.exists(self.session_file):
                with open(self.session_file, "r") as f:
                    data = json.load(f)
                    return data.get("message", ""), data.get("hint", "")
        except (json.JSONDecodeError, IOError):
            pass
        return "", ""

    def draw(self):
        width, height = get_term_size()
        self.width = width
        panel_height = 4
        panel_y = height - panel_height + 1

        msg, hint = self.get_content()

        position(panel_y, 1)
        clear_line()
        title = f" {self.session_name} | CORRIDOR "
        title_padded = title + " " * (width - len(title))
        sys.stdout.write(GREEN + BOLD + title_padded + RESET)
        sys.stdout.write("\r\n")
        clear_line()
        sys.stdout.flush()

        if msg:
            msg_lines = msg.split("\n")[:2]
        else:
            msg_lines = ["waiting for commands..."]

        for line in msg_lines:
            clear_line()
            if len(line) > width:
                line = line[: width - 3] + "..."
            sys.stdout.write(CYAN + line + RESET)
            sys.stdout.write("\r\n")
            clear_line()
            sys.stdout.flush()

        while len(msg_lines) < 2:
            clear_line()
            sys.stdout.write("\r\n")
            clear_line()
            sys.stdout.flush()
            msg_lines.append("")

        clear_line()
        if hint:
            if len(hint) > width:
                hint = hint[: width - 3] + "..."
            sys.stdout.write(GRAY + hint + RESET)
        sys.stdout.write("\r\n")
        clear_line()
        sys.stdout.flush()

        position(panel_y + panel_height, 1)
        sys.stdout.flush()

    def run(self):
        while self.running:
            self.draw()
            for _ in range(10):
                if not self.running:
                    break
                time.sleep(0.1)


def main():
    if len(sys.argv) > 1:
        session = sys.argv[1]
    else:
        session = input("Session name: ").strip()

    if not session:
        print("Session name required")
        sys.exit(1)

    app = CorridorDisplay(session)

    def signal_handler(sig, frame):
        app.running = False

    signal.signal(signal.SIGINT, signal_handler)

    try:
        app.run()
    except KeyboardInterrupt:
        pass


if __name__ == "__main__":
    main()
