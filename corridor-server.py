#!/usr/bin/env python3
import argparse
import json
import os
from http.server import HTTPServer, BaseHTTPRequestHandler
from urllib.parse import urlparse, parse_qs

SESSION_FILE = "/tmp/corridor-{session}.json"

HTML = """<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>Corridor</title>
    <style>
        * { box-sizing: border-box; margin: 0; padding: 0; }
        body {
            background: #0d1117;
            color: #c9d1d9;
            font-family: "JetBrains Mono", "Fira Code", monospace;
            min-height: 100vh;
            display: flex;
            flex-direction: column;
            align-items: center;
            padding: 40px 20px;
        }
        .container {
            width: 100%;
            max-width: 600px;
        }
        .header {
            display: flex;
            align-items: center;
            justify-content: space-between;
            margin-bottom: 30px;
            padding-bottom: 20px;
            border-bottom: 1px solid #30363d;
        }
        h1 {
            font-size: 24px;
            font-weight: 700;
            color: #58a6ff;
        }
        .session {
            color: #238636;
            font-size: 14px;
        }
        .session::before {
            content: "● ";
        }
        .form-group {
            margin-bottom: 20px;
        }
        label {
            display: block;
            margin-bottom: 8px;
            font-size: 12px;
            color: #8b949e;
            text-transform: uppercase;
            letter-spacing: 1px;
        }
        textarea, input[type="text"] {
            width: 100%;
            background: #0d1117;
            border: 1px solid #30363d;
            border-radius: 6px;
            color: #c9d1d9;
            font-family: inherit;
            font-size: 16px;
            padding: 12px;
            transition: border-color 0.2s;
        }
        textarea:focus, input[type="text"]:focus {
            outline: none;
            border-color: #58a6ff;
        }
        textarea {
            min-height: 100px;
            resize: vertical;
        }
        button {
            width: 100%;
            background: #238636;
            color: #fff;
            border: none;
            border-radius: 6px;
            padding: 14px;
            font-family: inherit;
            font-size: 14px;
            font-weight: 600;
            text-transform: uppercase;
            letter-spacing: 1px;
            cursor: pointer;
            transition: background 0.2s;
        }
        button:hover {
            background: #2ea043;
        }
        .hint-text {
            font-size: 12px;
            color: #8b949e;
            margin-top: 8px;
        }
        .last-message {
            margin-top: 20px;
            padding: 12px;
            background: #161b22;
            border: 1px solid #30363d;
            border-radius: 6px;
        }
        .last-message-label {
            font-size: 11px;
            color: #8b949e;
            text-transform: uppercase;
            letter-spacing: 1px;
            margin-bottom: 8px;
        }
        .last-message-text {
            color: #c9d1d9;
            font-family: inherit;
            font-size: 14px;
            word-break: break-all;
        }
    </style>
</head>
<body>
    <div class="container">
        <div class="header">
            <h1>CORRIDOR</h1>
            <span class="session" id="session"></span>
        </div>
        <div class="form-group">
            <label>Command / Message</label>
            <textarea id="message" placeholder="Enter message..."></textarea>
        </div>
        <button id="send">Send (Ctrl+Enter)</button>
        <div class="last-message">
            <div class="last-message-label">Last Message Sent</div>
            <div class="last-message-text" id="last-message"></div>
        </div>
    </div>
    <script>
        const session = new URLSearchParams(window.location.search).get('session') || 'default';
        document.getElementById('session').textContent = session;
        
        const msgEl = document.getElementById('message');
        const lastMsgEl = document.getElementById('last-message');
        
        async function loadLastMessage() {
            try {
                const resp = await fetch('/api/message');
                const data = await resp.json();
                lastMsgEl.textContent = data.message || '(none)';
            } catch (e) {
                lastMsgEl.textContent = '(none)';
            }
        }
        
        async function send() {
            const message = msgEl.value;
            if (!message) return;
            
            await fetch('/api/update', {
                method: 'POST',
                headers: {'Content-Type': 'application/json'},
                body: JSON.stringify({message})
            });
            
            msgEl.value = '';
            lastMsgEl.textContent = message;
        }
        
        loadLastMessage();
        
        document.getElementById('send').addEventListener('click', send);
        
        document.addEventListener('keydown', (e) => {
            if (e.ctrlKey && e.key === 'Enter') {
                send();
            }
        });
    </script>
</body>
</html>"""


class Handler(BaseHTTPRequestHandler):
    def do_GET(self):
        parsed = urlparse(self.path)
        if parsed.path == "/":
            params = parse_qs(parsed.query)
            session = params.get("session", ["default"])[0]
            html = HTML.replace(
                '<span class="session" id="session"></span>',
                f'<span class="session" id="session">{session}</span>',
            )
            self.send_response(200)
            self.send_header("Content-Type", "text/html")
            self.end_headers()
            self.wfile.write(html.encode())
        else:
            self.send_response(404)
            self.end_headers()

    def do_POST(self):
        if self.path == "/api/update":
            length = int(self.headers.get("Content-Length", 0))
            body = self.rfile.read(length)
            data = json.loads(body)

            params = parse_qs(urlparse(self.path).query)
            session = "default"

            parsed = urlparse(self.path)

            session = self.headers.get("X-Session", "default")

            content_length = int(self.headers.get("Content-Length", 0))
            if content_length > 0:
                body = self.rfile.read(content_length)
                try:
                    data = json.loads(body)
                except:
                    data = {}

            self.send_response(200)
            self.send_header("Content-Type", "application/json")
            self.end_headers()
            self.wfile.write(b'{"status":"ok"}')
        else:
            self.send_response(404)
            self.end_headers()

    def log_message(self, format, *args):
        pass


class SessionHandler(Handler):
    def __init__(self, *args, session="", **kwargs):
        self.session = session
        super().__init__(*args, **kwargs)

    def do_POST(self):
        if self.path == "/api/update":
            length = int(self.headers.get("Content-Length", 0))
            body = self.rfile.read(length)
            data = json.loads(body)

            filepath = SESSION_FILE.format(session=self.session)
            with open(filepath, "w") as f:
                f.write(data.get("message", ""))

            self.send_response(200)
            self.send_header("Content-Type", "application/json")
            self.end_headers()
            self.wfile.write(b'{"status":"ok"}')
        else:
            self.send_response(404)
            self.end_headers()


def make_handler(session):
    class CustomHandler(BaseHTTPRequestHandler):
        session_name = session

        def do_GET(self):
            parsed = urlparse(self.path)
            if parsed.path == "/":
                html = HTML.replace("session || 'default'", f"'{session}'")
                self.send_response(200)
                self.send_header("Content-Type", "text/html")
                self.end_headers()
                self.wfile.write(html.encode())
            elif parsed.path == "/api/message":
                filepath = SESSION_FILE.format(session=session)
                try:
                    with open(filepath, "r") as f:
                        message = f.read()
                except FileNotFoundError:
                    message = ""
                self.send_response(200)
                self.send_header("Content-Type", "application/json")
                self.end_headers()
                self.wfile.write(json.dumps({"message": message}).encode())
            else:
                self.send_response(404)
                self.end_headers()

        def do_POST(self):
            if self.path == "/api/update":
                length = int(self.headers.get("Content-Length", 0))
                body = self.rfile.read(length)
                data = json.loads(body)

                filepath = SESSION_FILE.format(session=session)
                with open(filepath, "w") as f:
                    f.write(data.get("message", ""))

                self.send_response(200)
                self.send_header("Content-Type", "application/json")
                self.end_headers()
                self.wfile.write(b'{"status":"ok"}')
            else:
                self.send_response(404)
                self.end_headers()

        def log_message(self, format, *args):
            pass

    return CustomHandler


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument(
        "--port", "-p", type=int, default=8080, help="Port to run server on"
    )
    args = parser.parse_args()

    session = os.environ.get("SESSION", "default")
    server = HTTPServer(("", args.port), make_handler(session))
    print(f"Corridor server running at http://localhost:{args.port}?session={session}")
    try:
        server.serve_forever()
    except KeyboardInterrupt:
        pass
    finally:
        server.server_close()


if __name__ == "__main__":
    main()
