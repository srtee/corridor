#!/usr/bin/env python3
import argparse
import curses
import json
import os
import pty
import select
import subprocess
import sys
import pyte
import urllib.request

SESSION_FILE = "/tmp/corridor-{session}.json"
PANEL_HEIGHT = 5

COLOR_MAP = {
    "black": 0,
    "red": 1,
    "green": 2,
    "yellow": 3,
    "blue": 4,
    "magenta": 5,
    "cyan": 6,
    "white": 7,
    "default": -1,
}

CURSES_COLOR_MAP = {
    0: curses.COLOR_BLACK,
    1: curses.COLOR_RED,
    2: curses.COLOR_GREEN,
    3: curses.COLOR_YELLOW,
    4: curses.COLOR_BLUE,
    5: curses.COLOR_MAGENTA,
    6: curses.COLOR_CYAN,
    7: curses.COLOR_WHITE,
}


def parse_args():
    parser = argparse.ArgumentParser()
    parser.add_argument("--session", "-s", default=None, help="Session name")
    parser.add_argument(
        "--url",
        "-u",
        default=None,
        help="Base URL (e.g., http://localhost:4000 or https://my.remote.url)",
    )
    parser.add_argument(
        "--debug",
        "-d",
        action="store_true",
        help="Enable debug output to stderr",
    )
    return parser.parse_args()


def wrap_text(text, width):
    lines = []
    for line in text.split("\n"):
        if len(line) <= width:
            lines.append(line)
        else:
            words = line.split()
            current = ""
            for word in words:
                if len(current) + len(word) + 1 <= width:
                    current += (" " if current else "") + word
                else:
                    if current:
                        lines.append(current)
                    current = word
            if current:
                lines.append(current)
    return lines


def read_session_data(session, url, debug=False):
    api_url = f"{url}/api/message?session={session}"
    try:
        req = urllib.request.Request(
            api_url, headers={"User-Agent": "curl/8.0.1", "Accept": "*/*"}
        )
        if api_url.startswith("https://"):
            import ssl

            ctx = ssl.create_default_context()
            ctx.check_hostname = False
            ctx.verify_mode = ssl.CERT_NONE
            with urllib.request.urlopen(req, timeout=5, context=ctx) as resp:
                data = json.loads(resp.read().decode())
                return data.get("message", ""), None
        else:
            with urllib.request.urlopen(req, timeout=5) as resp:
                data = json.loads(resp.read().decode())
                return data.get("message", ""), None
    except urllib.request.HTTPError as e:
        if debug:
            print(f"DEBUG: HTTP {e.code} for {api_url}", file=sys.stderr)
        return "", f"HTTP {e.code}"
    except Exception as e:
        if debug:
            print(f"DEBUG: fetch failed for {api_url}: {e}", file=sys.stderr)
        return "", str(e)


def main(stdscr):
    curses.curs_set(1)
    stdscr.nodelay(True)
    stdscr.keypad(True)

    curses.start_color()
    curses.use_default_colors()
    for i in range(1, 9):
        curses.init_pair(i, CURSES_COLOR_MAP[i - 1], -1)
    curses.init_pair(9, curses.COLOR_BLACK, curses.COLOR_WHITE)
    curses.init_pair(10, curses.COLOR_WHITE, curses.COLOR_BLACK)

    args = parse_args()
    session = args.session if args.session else os.environ.get("SESSION", "default")
    url = args.url if args.url else os.environ.get("URL", "http://localhost:8080")
    debug = args.debug

    master, slave = pty.openpty()
    shell = os.environ.get("SHELL", "/bin/bash")
    env = os.environ.copy()
    env["TERM"] = "xterm-256color"

    proc = subprocess.Popen(
        [shell],
        stdin=slave,
        stdout=slave,
        stderr=slave,
        start_new_session=True,
        env=env,
    )
    os.close(slave)

    screen = None
    stream = None
    last_error = None

    while True:
        h, w = stdscr.getmaxyx()
        main_h = h - PANEL_HEIGHT - 1

        if main_h < 2:
            break

        if screen is None or screen.columns != w or screen.lines != main_h:
            screen = pyte.Screen(w, main_h)

            original_sgr = screen.select_graphic_rendition

            def patched_sgr(*args, **kwargs):
                kwargs.pop("private", None)
                return original_sgr(*args, **kwargs)

            screen.select_graphic_rendition = patched_sgr

            stream = pyte.Stream(screen)

        try:
            ready, _, _ = select.select([master, sys.stdin], [], [], 0.1)
        except OSError:
            break

        if master in ready:
            try:
                data = os.read(master, 4096)
                if data:
                    stream.feed(data.decode("utf-8", errors="replace"))
            except OSError:
                break

        if proc.poll() is not None:
            break

        stdscr.erase()

        sep_y = main_h

        try:
            stdscr.addch(sep_y, 0, "├", curses.color_pair(6))
            session_text = f" {session} "
            text_start = max(1, (w - len(session_text)) // 2)
            stdscr.addstr(sep_y, 1, "─" * (text_start - 1), curses.color_pair(6))
            stdscr.addstr(
                sep_y, text_start, session_text, curses.A_BOLD | curses.color_pair(6)
            )
            stdscr.addstr(
                sep_y,
                text_start + len(session_text),
                "─" * (w - text_start - len(session_text) - 1),
                curses.color_pair(6),
            )
            stdscr.addch(sep_y, w - 1, "┤", curses.color_pair(6))
        except curses.error:
            pass

        for y in range(main_h):
            for x in range(w):
                line = screen.buffer.get(y, {})
                char_obj = line.get(x)
                if not char_obj:
                    continue

                char = char_obj.data if char_obj.data else " "

                fg = char_obj.fg
                bg = char_obj.bg

                attr = curses.A_NORMAL

                if char_obj.bold:
                    attr |= curses.A_BOLD
                if char_obj.underscore:
                    attr |= curses.A_UNDERLINE
                if char_obj.reverse:
                    attr |= curses.A_REVERSE
                if char_obj.blink:
                    attr |= curses.A_BLINK

                fg_color = COLOR_MAP.get(fg, 7)
                if fg_color != 7:
                    attr |= curses.color_pair(fg_color + 1)

                if bg != "default":
                    bg_color = COLOR_MAP.get(bg, -1)
                    if bg_color >= 0:
                        stdscr.addstr(
                            y,
                            x,
                            char,
                            curses.color_pair(bg_color + 1) | curses.A_REVERSE,
                        )
                        continue

                try:
                    stdscr.addstr(y, x, char, attr)
                except curses.error:
                    pass

        session_data, fetch_error = read_session_data(session, url, debug)
        if fetch_error:
            last_error = fetch_error
        web_start = main_h + 1
        web_lines = 4

        if session_data:
            wrapped = wrap_text(session_data, w - 1)[:web_lines]
            for i, line in enumerate(wrapped):
                try:
                    stdscr.addstr(web_start + i, 0, line, curses.A_BOLD)
                except curses.error:
                    pass
        elif last_error:
            try:
                stdscr.addstr(web_start, 0, f"[{last_error}]", curses.color_pair(1))
            except curses.error:
                pass
        else:
            placeholder = "[web panel empty - send data via web interface]"
            try:
                stdscr.addstr(web_start, 0, placeholder, curses.color_pair(6))
            except curses.error:
                pass

        stdscr.move(0, 0)
        stdscr.refresh()

        try:
            key = stdscr.getch()
            if key != -1 and key < 256:
                try:
                    os.write(master, bytes([key]))
                except OSError:
                    break
            elif key == curses.KEY_BACKSPACE:
                try:
                    os.write(master, b"\x7f")
                except OSError:
                    break
            elif key in (curses.KEY_ENTER, 10, 13):
                try:
                    os.write(master, b"\n")
                except OSError:
                    break
            elif key == curses.KEY_UP:
                try:
                    os.write(master, b"\x1b[A")
                except OSError:
                    break
            elif key == curses.KEY_DOWN:
                try:
                    os.write(master, b"\x1b[B")
                except OSError:
                    break
            elif key == curses.KEY_RIGHT:
                try:
                    os.write(master, b"\x1b[C")
                except OSError:
                    break
            elif key == curses.KEY_LEFT:
                try:
                    os.write(master, b"\x1b[D")
                except OSError:
                    break
            elif key == 27:
                try:
                    os.write(master, b"\x1b")
                except OSError:
                    break
        except curses.error:
            pass

    os.close(master)
    proc.terminate()
    try:
        proc.wait(timeout=1)
    except:
        proc.kill()


if __name__ == "__main__":
    try:
        curses.wrapper(main)
    except KeyboardInterrupt:
        pass
