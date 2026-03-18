#!/usr/bin/env python3
import argparse
import json
import os
import re
import glob
import time
import threading
from http.server import HTTPServer, BaseHTTPRequestHandler
from urllib.parse import urlparse, parse_qs

SESSION_FILE = "/tmp/corridor-{session}.json"
SESSION_PATTERN = re.compile(r"^[a-zA-Z0-9_-]{1,16}$")
SESSION_TIMEOUT = 3600
CLEANUP_INTERVAL = 300


def sanitize_session_name(name):
    if not name:
        return "default"
    name = str(name).strip()
    if not SESSION_PATTERN.match(name):
        return None
    if name.startswith(".") or name.startswith("-"):
        return None
    return name


def sanitize_message(msg):
    if msg is None:
        return ""
    return str(msg).rstrip()[:600]


def read_session_data(filepath):
    try:
        with open(filepath, "r") as f:
            data = json.load(f)
            return data.get("message", ""), data.get("last_access", time.time())
    except (FileNotFoundError, json.JSONDecodeError):
        return "", None


def write_session_data(filepath, message):
    data = {"message": sanitize_message(message), "last_access": time.time()}
    with open(filepath, "w") as f:
        json.dump(data, f)


def cleanup_expired_sessions():
    now = time.time()
    for filepath in glob.glob("/tmp/corridor-*.json"):
        try:
            with open(filepath, "r") as f:
                data = json.load(f)
            last_access = data.get("last_access", 0)
            if now - last_access > SESSION_TIMEOUT:
                os.remove(filepath)
        except (FileNotFoundError, json.JSONDecodeError, OSError):
            pass


def cleanup_loop():
    while True:
        time.sleep(CLEANUP_INTERVAL)
        cleanup_expired_sessions()


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
            cursor: pointer;
            position: relative;
            padding: 4px 8px;
            border-radius: 4px;
            transition: background 0.2s;
            background: none;
            border: none;
            font-family: inherit;
        }
        .session::before {
            content: "● ";
        }
        .session:hover, .session:focus {
            background: #21262d;
            outline: none;
        }
        .session:focus-visible {
            outline: 2px solid #58a6ff;
            outline-offset: 2px;
        }
        .session::after {
            content: " ▼";
            font-size: 10px;
            color: #8b949e;
        }
        .session-dropdown {
            display: none;
            position: absolute;
            top: 100%;
            right: 0;
            margin-top: 8px;
            min-width: 200px;
            background: #161b22;
            border: 1px solid #30363d;
            border-radius: 6px;
            padding: 8px;
            z-index: 100;
            box-shadow: 0 8px 24px rgba(0,0,0,0.4);
        }
        .session-dropdown.open {
            display: block;
        }
        .new-session-form {
            padding: 8px;
            border-bottom: 1px solid #30363d;
            margin-bottom: 8px;
        }
        .new-session-form input {
            width: 100%;
            padding: 6px 8px;
            font-size: 12px;
            margin-bottom: 4px;
        }
        .new-session-form button {
            width: 100%;
            padding: 6px 12px;
            font-size: 11px;
        }
        .new-session-form .hint-text {
            font-size: 10px;
            margin-bottom: 4px;
        }
        .session-list {
            max-height: 200px;
            overflow-y: auto;
        }
        .session-item {
            display: flex;
            align-items: center;
            justify-content: space-between;
            width: 100%;
            padding: 8px 10px;
            font-size: 13px;
            background: transparent;
            border: none;
            border-radius: 4px;
            color: #c9d1d9;
            text-align: left;
            cursor: pointer;
            transition: background 0.15s;
        }
        .session-item:hover {
            background: #21262d;
        }
        .session-item.active {
            color: #238636;
        }
        .session-item.active::before {
            content: "● ";
            padding-right: 6px;
        }
        .session-item-name {
            flex: 1;
            min-width: 0;
            background: none;
            border: none;
            color: inherit;
            font-size: inherit;
            font-family: inherit;
            text-align: left;
            cursor: pointer;
            padding: 0;
        }
        .session-item-name:focus-visible {
            outline: 2px solid #58a6ff;
            outline-offset: 2px;
        }
        .session-delete {
            opacity: 0;
            background: none;
            border: none;
            color: #f85149;
            cursor: pointer;
            padding: 2px 6px;
            font-size: 14px;
            border-radius: 3px;
            transition: opacity 0.15s, background 0.15s;
            width: 24px;
            text-align: center;
            flex-shrink: 0;
        }
        .session-item:hover .session-delete {
            opacity: 1;
        }
        .session-delete:hover {
            background: #f8514933;
        }
        .session-delete:focus-visible {
            outline: 2px solid #58a6ff;
            outline-offset: 2px;
        }
        .session-item.active .session-delete {
            display: none;
        }
        .error {
            color: #f85149;
            font-size: 11px;
            margin-top: 4px;
        }
        .sr-only {
            position: absolute;
            width: 1px;
            height: 1px;
            padding: 0;
            margin: -1px;
            overflow: hidden;
            clip: rect(0, 0, 0, 0);
            white-space: nowrap;
            border: 0;
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
        textarea:focus-visible, input[type="text"]:focus-visible {
            outline: 2px solid #58a6ff;
            outline-offset: 2px;
        }
        button:focus-visible {
            outline: 2px solid #58a6ff;
            outline-offset: 2px;
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
            <div style="position: relative;">
                <button class="session" id="session-btn" aria-haspopup="listbox" aria-expanded="false" aria-controls="session-dropdown"></button>
                <div class="session-dropdown" id="session-dropdown" role="listbox" aria-label="Session selector">
                    <div class="new-session-form" role="form" aria-label="Create new session">
                        <label for="new-session-name" class="sr-only">New session name</label>
                        <input type="text" id="new-session-name" placeholder="New session name" maxlength="16" aria-describedby="session-hint">
                        <button id="create-session">Create Session</button>
                        <div class="hint-text" id="session-hint">a-z, A-Z, 0-9, _, - (max 16 chars)</div>
                        <div class="error" id="error-msg" role="alert" aria-live="polite"></div>
                    </div>
                    <div class="session-list" id="session-list" role="group" aria-label="Available sessions"></div>
                </div>
            </div>
        </div>
        <div class="form-group">
            <label for="message">Command / Message</label>
            <textarea id="message" placeholder="Enter message..." aria-describedby="send-hint" maxlength="600"></textarea>
        </div>
        <button id="send" aria-describedby="send-hint">Send</button>
        <div class="hint-text" id="send-hint" style="margin-top: 4px;">Ctrl+Enter to send</div>
        <div class="last-message" aria-live="polite">
            <div class="last-message-label" id="last-msg-label">Last Message Sent</div>
            <div class="last-message-text" id="last-message" aria-labelledby="last-msg-label"></div>
        </div>
    </div>
    <script>
        let currentSession = new URLSearchParams(window.location.search).get('session') || 'default';
        const sessionBtn = document.getElementById('session-btn');
        const sessionDropdown = document.getElementById('session-dropdown');
        sessionBtn.textContent = currentSession;
        
        const msgEl = document.getElementById('message');
        const lastMsgEl = document.getElementById('last-message');
        const sessionListEl = document.getElementById('session-list');
        const newSessionInput = document.getElementById('new-session-name');
        const errorMsgEl = document.getElementById('error-msg');
        
        function openDropdown() {
            sessionDropdown.classList.add('open');
            sessionBtn.setAttribute('aria-expanded', 'true');
            newSessionInput.focus();
        }
        
        function closeDropdown() {
            sessionDropdown.classList.remove('open');
            sessionBtn.setAttribute('aria-expanded', 'false');
            sessionBtn.focus();
        }
        
        sessionBtn.addEventListener('click', (e) => {
            e.stopPropagation();
            if (sessionDropdown.classList.contains('open')) {
                closeDropdown();
            } else {
                openDropdown();
            }
        });
        
        sessionBtn.addEventListener('keydown', (e) => {
            if (e.key === 'Enter' || e.key === ' ') {
                e.preventDefault();
                if (sessionDropdown.classList.contains('open')) {
                    closeDropdown();
                } else {
                    openDropdown();
                }
            }
        });
        
        document.addEventListener('click', (e) => {
            if (!sessionDropdown.contains(e.target) && e.target !== sessionBtn) {
                sessionDropdown.classList.remove('open');
                sessionBtn.setAttribute('aria-expanded', 'false');
            }
        });
        
        document.addEventListener('keydown', (e) => {
            if (e.key === 'Escape' && sessionDropdown.classList.contains('open')) {
                closeDropdown();
            }
        });
        
        function showError(msg) {
            errorMsgEl.textContent = msg;
            setTimeout(() => errorMsgEl.textContent = '', 3000);
        }
        
        async function loadSessions() {
            try {
                const resp = await fetch('/api/sessions');
                const data = await resp.json();
                renderSessionList(data.sessions || []);
            } catch (e) {
                renderSessionList([currentSession]);
            }
        }
        
        function renderSessionList(sessions) {
            sessionListEl.innerHTML = '';
            sessions.forEach((s, index) => {
                const item = document.createElement('div');
                item.className = 'session-item' + (s === currentSession ? ' active' : '');
                item.setAttribute('role', 'option');
                item.setAttribute('aria-selected', s === currentSession ? 'true' : 'false');
                
                const nameSpan = document.createElement('button');
                nameSpan.className = 'session-item-name';
                nameSpan.textContent = s;
                nameSpan.setAttribute('aria-label', 'Switch to session ' + s);
                if (s === currentSession) {
                    nameSpan.setAttribute('aria-current', 'true');
                }
                nameSpan.onclick = () => switchSession(s);
                
                const deleteBtn = document.createElement('button');
                deleteBtn.className = 'session-delete';
                deleteBtn.innerHTML = '×';
                deleteBtn.setAttribute('aria-label', 'Delete session ' + s);
                deleteBtn.onclick = (e) => {
                    e.stopPropagation();
                    deleteSession(s);
                };
                
                item.appendChild(nameSpan);
                item.appendChild(deleteBtn);
                sessionListEl.appendChild(item);
            });
        }
        
        async function deleteSession(name) {
            if (name === currentSession) {
                showError('Cannot delete current session');
                return;
            }
            if (!confirm('Delete session "' + name + '"?')) return;
            
            try {
                const resp = await fetch('/api/session?name=' + encodeURIComponent(name), {
                    method: 'DELETE'
                });
                if (resp.ok) {
                    loadSessions();
                } else {
                    showError('Failed to delete session');
                }
            } catch (e) {
                showError('Error deleting session');
            }
        }
        
        function switchSession(name) {
            window.location.href = '/?session=' + encodeURIComponent(name);
        }
        
        async function createSession() {
            const name = newSessionInput.value.trim();
            if (!name) {
                showError('Please enter a session name');
                return;
            }
            if (!/^[a-zA-Z0-9_-]{1,16}$/.test(name)) {
                showError('Invalid: use a-z, 0-9, _, - only');
                return;
            }
            try {
                await fetch('/api/update?session=' + encodeURIComponent(name), {
                    method: 'POST',
                    headers: {'Content-Type': 'application/json'},
                    body: JSON.stringify({message: ''})
                });
            } catch (e) {}
            switchSession(name);
        }
        
        async function loadLastMessage() {
            try {
                const resp = await fetch('/api/message?session=' + encodeURIComponent(currentSession));
                const data = await resp.json();
                lastMsgEl.textContent = data.message || '(none)';
            } catch (e) {
                lastMsgEl.textContent = '(none)';
            }
        }
        
        async function send() {
            const message = msgEl.value;
            if (!message) return;
            
            await fetch('/api/update?session=' + encodeURIComponent(currentSession), {
                method: 'POST',
                headers: {'Content-Type': 'application/json'},
                body: JSON.stringify({message})
            });
            
            msgEl.value = '';
            lastMsgEl.textContent = message;
        }
        
        loadSessions();
        loadLastMessage();
        
        document.getElementById('send').addEventListener('click', send);
        document.getElementById('create-session').addEventListener('click', createSession);
        
        newSessionInput.addEventListener('keydown', (e) => {
            if (e.key === 'Enter') createSession();
        });
        
        document.addEventListener('keydown', (e) => {
            if (e.ctrlKey && e.key === 'Enter') {
                send();
            }
        });
    </script>
</body>
</html>"""


def make_handler(session):
    class CustomHandler(BaseHTTPRequestHandler):
        session_name = session

        def get_session(self):
            parsed = urlparse(self.path)
            params = parse_qs(parsed.query)
            raw_session = params.get("session", [self.session_name])[0]
            sanitized = sanitize_session_name(raw_session)
            return sanitized if sanitized else "default"

        def do_GET(self):
            parsed = urlparse(self.path)
            sess = self.get_session()
            if parsed.path == "/":
                html = HTML.replace("session || 'default'", f"'{sess}'")
                self.send_response(200)
                self.send_header("Content-Type", "text/html")
                self.end_headers()
                self.wfile.write(html.encode())
            elif parsed.path == "/api/message":
                filepath = SESSION_FILE.format(session=sess)
                message, _ = read_session_data(filepath)
                write_session_data(filepath, message)
                self.send_response(200)
                self.send_header("Content-Type", "application/json")
                self.end_headers()
                self.wfile.write(json.dumps({"message": message}).encode())
            elif parsed.path == "/api/sessions":
                sessions = []
                now = time.time()
                for f in glob.glob("/tmp/corridor-*.json"):
                    name = (
                        os.path.basename(f)
                        .replace("corridor-", "")
                        .replace(".json", "")
                    )
                    if SESSION_PATTERN.match(name):
                        try:
                            with open(f, "r") as fh:
                                data = json.load(fh)
                            last_access = data.get("last_access", now)
                            if now - last_access <= SESSION_TIMEOUT:
                                sessions.append(name)
                        except (json.JSONDecodeError, FileNotFoundError):
                            sessions.append(name)
                if "default" not in sessions:
                    sessions.insert(0, "default")
                self.send_response(200)
                self.send_header("Content-Type", "application/json")
                self.end_headers()
                self.wfile.write(json.dumps({"sessions": sorted(sessions)}).encode())
            else:
                self.send_response(404)
                self.end_headers()

        def do_POST(self):
            parsed = urlparse(self.path)
            if parsed.path == "/api/update":
                sess = self.get_session()
                length = int(self.headers.get("Content-Length", 0))
                body = self.rfile.read(length)
                try:
                    data = json.loads(body)
                except json.JSONDecodeError:
                    data = {}
                message = sanitize_message(data.get("message", ""))

                filepath = SESSION_FILE.format(session=sess)
                write_session_data(filepath, message)

                self.send_response(200)
                self.send_header("Content-Type", "application/json")
                self.end_headers()
                self.wfile.write(b'{"status":"ok"}')
            else:
                self.send_response(404)
                self.end_headers()

        def do_DELETE(self):
            parsed = urlparse(self.path)
            if parsed.path == "/api/session":
                params = parse_qs(parsed.query)
                raw_name = params.get("name", [""])[0]
                name = sanitize_session_name(raw_name)
                if not name:
                    self.send_response(400)
                    self.send_header("Content-Type", "application/json")
                    self.end_headers()
                    self.wfile.write(b'{"error":"Invalid session name"}')
                    return

                filepath = SESSION_FILE.format(session=name)
                try:
                    os.remove(filepath)
                    self.send_response(200)
                    self.send_header("Content-Type", "application/json")
                    self.end_headers()
                    self.wfile.write(b'{"status":"deleted"}')
                except FileNotFoundError:
                    self.send_response(404)
                    self.send_header("Content-Type", "application/json")
                    self.end_headers()
                    self.wfile.write(b'{"error":"Session not found"}')
                except OSError:
                    self.send_response(500)
                    self.send_header("Content-Type", "application/json")
                    self.end_headers()
                    self.wfile.write(b'{"error":"Failed to delete session"}')
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

    cleanup_thread = threading.Thread(target=cleanup_loop, daemon=True)
    cleanup_thread.start()

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
