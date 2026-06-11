"""
Sticky Notes Context Classifier & Window Monitor Server
--------------------------------------------------------
1. HTTP server that classifies todo text into app/website contexts
   using Groq AI API.
2. Background window monitor that detects the active window title
   and browser URL (for Firefox/Chrome/Edge).

Endpoints:
  POST /classify  - {"text": "task description"} → {"contexts": ["app1", "app2"]}
  GET  /health    - {"status": "ok"}
"""

import json
import sys
import os
import re
import threading
import time
from http.server import HTTPServer, BaseHTTPRequestHandler
from dotenv import load_dotenv

# ─── LOAD .ENV ─────────────────────────────────────────────────────────
load_dotenv(os.path.join(os.path.dirname(os.path.abspath(__file__)), ".env"))

# ─── CONFIG ────────────────────────────────────────────────────────────
HOST = "127.0.0.1"
PORT = 8765
GROQ_API_KEY = os.environ.get("GROQ_API_KEY", "")
MODEL = "llama-3.3-70b-versatile"
POLL_INTERVAL = 1  # seconds between window checks

# ─── GROQ CLIENT ───────────────────────────────────────────────────────
import urllib.request

if not GROQ_API_KEY:
    print("ERROR: GROQ_API_KEY not set. Add it to src-tauri/.env", file=sys.stderr)
    sys.exit(1)

GROQ_URL = "https://api.groq.com/openai/v1/chat/completions"

# ─── WINDOW DETECTION ──────────────────────────────────────────────────
try:
    import win32gui
    import win32process
    import psutil
    import uiautomation as auto
    HAS_WINDOW_DEPS = True
except ImportError:
    HAS_WINDOW_DEPS = False
    print("[monitor] Optional deps missing. Run: pip install pywin32 psutil uiautomation", file=sys.stderr)


def get_active_window():
    """Returns (window_title, process_name, hwnd) of the foreground window."""
    hwnd = win32gui.GetForegroundWindow()
    if not hwnd:
        return None, None, None

    window_title = win32gui.GetWindowText(hwnd)
    if not window_title.strip():
        return None, None, None

    try:
        _, pid = win32process.GetWindowThreadProcessId(hwnd)
        process = psutil.Process(pid)
        process_name = process.name().lower()
    except (psutil.NoSuchProcess, psutil.AccessDenied, OSError):
        process_name = "unknown"

    return window_title, process_name, hwnd


def get_firefox_url(hwnd):
    try:
        window = auto.ControlFromHandle(hwnd)
        nav_bar = window.ToolBarControl(AutomationId='nav-bar')
        urlbar = nav_bar.GroupControl(AutomationId='urlbar')
        combo = urlbar.ComboBoxControl(AutomationId='urlbar-input')
        if combo.Exists(maxSearchSeconds=0.5):
            return combo.GetValuePattern().Value
    except Exception:
        pass
    return None


def get_chrome_url(hwnd):
    try:
        window = auto.ControlFromHandle(hwnd)
        addr = window.EditControl(Name="Address and search bar")
        if addr.Exists(maxSearchSeconds=0.5):
            return addr.GetValuePattern().Value
    except Exception:
        pass
    return None


def get_edge_url(hwnd):
    try:
        window = auto.ControlFromHandle(hwnd)
        addr = window.EditControl(Name="Address and search bar")
        if addr.Exists(maxSearchSeconds=0.5):
            return addr.GetValuePattern().Value
    except Exception:
        pass
    return None


# ─── CONTEXT DETECTION ─────────────────────────────────────────────────

# Context name → URL keywords
CONTEXT_URL_MAP = {
    "linkedin":       ["linkedin.com"],
    "github":         ["github.com"],
    "gmail":          ["mail.google.com", "gmail.com"],
    "youtube":        ["youtube.com", "youtu.be"],
    "leetcode":       ["leetcode.com"],
    "notion":         ["notion.so", "notion.site"],
    "chatgpt":        ["chatgpt.com", "chat.openai.com"],
    "google docs":    ["docs.google.com/document"],
    "google sheets":  ["docs.google.com/spreadsheets"],
    "google slides":  ["docs.google.com/presentation"],
    "google drive":   ["drive.google.com"],
    "twitter":        ["twitter.com", "x.com"],
    "instagram":      ["instagram.com"],
    "reddit":         ["reddit.com"],
    "stackoverflow":  ["stackoverflow.com"],
    "trello":         ["trello.com"],
    "jira":           ["atlassian.net", "jira.com"],
    "figma":          ["figma.com"],
    "vercel":         ["vercel.com"],
    "netlify":        ["netlify.com"],
    "heroku":         ["heroku.com"],
    "aws":            ["aws.amazon.com", "console.aws.amazon.com"],
    "google calendar":["calendar.google.com"],
    "slack":          ["slack.com"],
    "discord":        ["discord.com/app"],
}

# Process name → context name
CONTEXT_PROCESS_MAP = {
    "code.exe":             "vscode",
    "code":                 "vscode",
    "slack.exe":            "slack",
    "slack":                "slack",
    "discord.exe":          "discord",
    "discord":              "discord",
    "zoom.exe":             "zoom",
    "zoom":                 "zoom",
    "teams.exe":            "microsoft teams",
    "teams":                "microsoft teams",
    "outlook.exe":          "outlook",
    "spotify.exe":          "spotify",
    "obsidian.exe":         "obsidian",
    "notion.exe":           "notion",
    "figma.exe":            "figma",
    "postman.exe":          "postman",
    "pycharm64.exe":        "pycharm",
    "idea64.exe":           "intellij",
    "webstorm64.exe":       "webstorm",
    "androidstudio64.exe":  "android studio",
}

# Process names that belong to our own app — ignore these for context detection
OWN_APP_PROCESSES = {"sticky-notes.exe", "sticky_notes.exe"}

# ─── SLASH COMMANDS ──────────────────────────────────────────────────────
# Automatically build a map of valid slash commands from our existing context maps
SLASH_COMMAND_MAP = {}

# Map URL contexts (e.g., "google docs" -> /googledocs)
for ctx_name in CONTEXT_URL_MAP.keys():
    clean_name = ctx_name.replace(" ", "").lower()
    SLASH_COMMAND_MAP[clean_name] = ctx_name
    SLASH_COMMAND_MAP[ctx_name.lower()] = ctx_name

# Map Process contexts (e.g., "vscode" -> /vscode)
for ctx_name in set(CONTEXT_PROCESS_MAP.values()):
    clean_name = ctx_name.replace(" ", "").lower()
    SLASH_COMMAND_MAP[clean_name] = ctx_name
    SLASH_COMMAND_MAP[ctx_name.lower()] = ctx_name


# ─── APP DATA HELPERS ──────────────────────────────────────────────────

def _get_app_data_dir():
    """Find the Tauri app data directory where store files live."""
    import pathlib
    appdata = os.environ.get("APPDATA")
    if appdata:
        store_dir = os.path.join(appdata, "com.sticky-notes.app")
        if os.path.exists(store_dir):
            return store_dir
    return None


def load_todos():
    """Load all todos from todos.json."""
    store_dir = _get_app_data_dir()
    if not store_dir:
        return []
    path = os.path.join(store_dir, "todos.json")
    try:
        with open(path, "r", encoding="utf-8") as f:
            data = json.load(f)
        return data.get("todos", [])
    except (FileNotFoundError, json.JSONDecodeError):
        return []


def load_contexts():
    """Load context mapping from contexts.json."""
    store_dir = _get_app_data_dir()
    if not store_dir:
        return {}
    path = os.path.join(store_dir, "contexts.json")
    try:
        with open(path, "r", encoding="utf-8") as f:
            data = json.load(f)
        return data.get("contexts", {})
    except (FileNotFoundError, json.JSONDecodeError):
        return {}


def _get_all_saved_contexts() -> set:
    """Return all context names currently stored in contexts.json."""
    store_dir = _get_app_data_dir()
    if not store_dir:
        return set()
    path = os.path.join(store_dir, "contexts.json")
    try:
        with open(path, "r", encoding="utf-8") as f:
            data = json.load(f)
        return set(data.get("contexts", {}).keys())
    except (FileNotFoundError, json.JSONDecodeError):
        return set()


def get_todos_for_contexts(active_contexts):
    """Return todos whose contexts overlap with active_contexts."""
    todos = load_todos()
    contexts_map = load_contexts()

    if not active_contexts:
        return []

    active_lower = [c.lower() for c in active_contexts]

    # Build a map of todo_id → list of contexts from contexts.json
    todo_contexts = {}
    for context_name, todo_ids in contexts_map.items():
        for tid in todo_ids:
            todo_contexts.setdefault(tid, []).append(context_name)

    # Find matching todos
    matched = []
    for todo in todos:
        tid = todo["id"]
        todo_ctx = todo_contexts.get(tid, [])
        for tc in todo_ctx:
            tc_lower = tc.lower()
            if any(tc_lower in ac or ac in tc_lower for ac in active_lower):
                todo_with_ctx = dict(todo)
                todo_with_ctx["contexts"] = todo_ctx
                matched.append(todo_with_ctx)
                break

    return matched


# ─── CONTEXT DETECTION ─────────────────────────────────────────────────

def detect_current_contexts(title, process, hwnd):
    """Returns (list_of_contexts, url) for the current active window."""
    contexts = set()

    # URL-based contexts (for browsers)
    url = None
    if "firefox" in process:
        url = get_firefox_url(hwnd)
    elif "chrome" in process:
        url = get_chrome_url(hwnd)
    elif "msedge" in process or "edge" in process:
        url = get_edge_url(hwnd)

    if url:
        url_lower = url.lower()
        for context_name, patterns in CONTEXT_URL_MAP.items():
            for pattern in patterns:
                if pattern in url_lower:
                    contexts.add(context_name)
                    break

    # Browser fallback if no URL context was matched
    if url == "":
        if "firefox" in process:
            contexts.add("firefox")
        elif "chrome" in process:
            contexts.add("chrome")
        elif "msedge" in process or "edge" in process:
            contexts.add("edge")

    # Process-based contexts
    for proc_key, ctx_name in CONTEXT_PROCESS_MAP.items():
        if proc_key in process:
            contexts.add(ctx_name)
            break

    # ─── FUZZY MATCH: unknown slash-command contexts ────────────────────
    # Any context saved in contexts.json that isn't in the known maps gets
    # matched by searching its name inside the process name, window title,
    # or active URL — so /myapp works without any map entry.
    known_contexts = set(CONTEXT_URL_MAP.keys()) | set(CONTEXT_PROCESS_MAP.values())
    all_saved_contexts = _get_all_saved_contexts()

    title_lower = title.lower() if title else ""
    url_lower = (url or "").lower()
    process_lower = process.lower() if process else ""

    for ctx in all_saved_contexts:
        if ctx in known_contexts:
            continue  # already handled above
        ctx_lower = ctx.lower()
        if (ctx_lower in process_lower or
                ctx_lower in title_lower or
                ctx_lower in url_lower):
            contexts.add(ctx)
            print(f"  [monitor] ✅ Fuzzy matched unknown context '{ctx}' in window signal", flush=True)

    return list(contexts), url


# ─── WINDOW MONITOR ────────────────────────────────────────────────────

def monitor_loop():
    """Background thread that polls the active window and sends reminders to Rust."""
    if not HAS_WINDOW_DEPS:
        return

    import comtypes
    comtypes.CoInitialize()

    import urllib.request

    REMINDER_URL = "http://127.0.0.1:8766/remind"
    SLIDE_LEFT_URL = "http://127.0.0.1:8766/slide-left"

    last_title = None
    last_url = None
    last_process = None
    last_contexts = []
    last_todo_ids = []

    print("\n🔍 Window monitor active — watching active window...")

    while True:
        try:
            title, process, hwnd = get_active_window()

            if title is None or process is None:
                time.sleep(POLL_INTERVAL)
                continue

            is_own_window = process in OWN_APP_PROCESSES
            if is_own_window:
                time.sleep(POLL_INTERVAL)
                continue

            active_contexts, url = detect_current_contexts(title, process, hwnd)

            context_changed = set(active_contexts) != set(last_contexts)

            if context_changed:
                try:
                    req = urllib.request.Request(
                        SLIDE_LEFT_URL,
                        data=b"{}",
                        headers={"Content-Type": "application/json"},
                        method="POST",
                    )
                    urllib.request.urlopen(req, timeout=2)
                except Exception:
                    pass
                time.sleep(0.35)
                last_contexts = active_contexts
                last_todo_ids = []

            matched_todos = get_todos_for_contexts(active_contexts) if active_contexts else []
            current_todo_ids = [t["id"] for t in matched_todos]
            todos_changed = current_todo_ids != last_todo_ids

            title_changed = title != last_title
            url_changed = url != last_url
            process_changed = process != last_process

            if title_changed or url_changed or process_changed:
                display_url = f" | URL: {url[:80]}" if url else ""
                print(f"\n{'─' * 55}", flush=True)
                print(f"  🪟 [{process}] {title}{display_url}", flush=True)

                if active_contexts:
                    ctx_str = ", ".join(active_contexts)
                    print(f"  📍 Context: {ctx_str}", flush=True)

                    if matched_todos:
                        print(f"  📋 Your todos for this context:", flush=True)
                        for todo in matched_todos:
                            done = "✅" if todo.get("status") == "done" else "☐"
                            task = todo.get("task", "??")
                            ctx_tags = ", ".join(todo.get("contexts", ["general"]))
                            print(f"     {done} {task}  [{ctx_tags}]", flush=True)
                    else:
                        print(f"  📋 No todos for this context", flush=True)

                print(f"{'─' * 55}\n", flush=True)

                last_title = title
                last_url = url
                last_process = process

            if context_changed and matched_todos:
                undone_todos = [t for t in matched_todos if t.get("status") != "done"]
                if undone_todos:
                    time.sleep(0.35)
                    try:
                        ctx_str = ", ".join(active_contexts) if active_contexts else ""
                        payload = json.dumps({"todos": undone_todos, "context": ctx_str}).encode()
                        req = urllib.request.Request(
                            REMINDER_URL,
                            data=payload,
                            headers={"Content-Type": "application/json"},
                            method="POST",
                        )
                        urllib.request.urlopen(req, timeout=2)
                    except Exception as e:
                        print(f"  [reminder] Failed to send: {e}", flush=True)

            last_todo_ids = current_todo_ids

        except Exception as e:
            print(f"  [monitor error] {e}", flush=True)

        time.sleep(POLL_INTERVAL)


# ─── CLASSIFIER ────────────────────────────────────────────────────────
CLASSIFY_PROMPT = """
You are a task-context prediction engine.

Return ONLY valid JSON in this format:
{{ "contexts": ["app_or_website_name"] }}

Rules:
- Focus on the most likely execution environment
- Be specific (e.g., "linkedin", "gmail", "github", "leetcode", "notion", "vscode", "youtube", "chatgpt", "google docs", "figma", "slack", "discord", "calendar")
- Include up to 1 contexts max 

- Do NOT add markdown, text, or reasoning
- Output must be valid JSON

Task: {text}
"""


def classify_todo(text: str) -> list[str]:
    # 1. CHECK FOR SLASH COMMANDS FIRST
    match = re.search(r'(?:^|\s)/([a-zA-Z0-9_]+)', text)

    if match:
        cmd = match.group(1).lower()

        # Known command → map to canonical name
        if cmd in SLASH_COMMAND_MAP:
            context_name = SLASH_COMMAND_MAP[cmd]
            print(f"  [classify] ⚡ Slash command detected: /{cmd} -> {context_name}", flush=True)
            return [context_name]

        # ✅ Unknown command → save the raw word as context directly
        else:
            print(f"  [classify] ⚡ Unknown slash command: /{cmd} -> saving as-is", flush=True)
            return [cmd]

    # 2. FALLBACK TO GROQ AI
    prompt = CLASSIFY_PROMPT.format(text=text)
    payload = json.dumps({
        "model": MODEL,
        "messages": [{"role": "user", "content": prompt}],
        "temperature": 0,
    }).encode("utf-8")

    try:
        req = urllib.request.Request(
            GROQ_URL,
            data=payload,
            headers={
                "Content-Type": "application/json",
                "Authorization": f"Bearer {GROQ_API_KEY}",
                "User-Agent": "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36",
            },
            method="POST",
        )
        resp = urllib.request.urlopen(req, timeout=10)
        body = json.loads(resp.read().decode("utf-8"))
        content = body["choices"][0]["message"]["content"]
        print(f"  [classify] Raw response: {content!r}", flush=True)

        try:
            data = json.loads(content)
            return data.get("contexts", ["general"])
        except Exception:
            cleaned = content.strip().replace("```json", "").replace("```", "").strip()
            data = json.loads(cleaned)
            return data.get("contexts", ["general"])
    except Exception as e:
        print(f"  [classify error] {e}", flush=True)
        return ["general"]


# ─── HTTP SERVER ───────────────────────────────────────────────────────
class Handler(BaseHTTPRequestHandler):
    def do_GET(self):
        if self.path == "/health":
            self._json(200, {"status": "ok"})
        else:
            self._json(404, {"error": "not found"})

    def do_POST(self):
        if self.path != "/classify":
            self._json(404, {"error": "not found"})
            return

        content_length = int(self.headers.get("Content-Length", 0))
        body = self.rfile.read(content_length)
        try:
            data = json.loads(body)
            text = data.get("text", "")
        except (json.JSONDecodeError, KeyError):
            self._json(400, {"error": "invalid json, expected {\"text\": \"...\"}"})
            return

        if not text.strip():
            self._json(400, {"error": "text is required"})
            return

        try:
            contexts = classify_todo(text)
            self._json(200, {"contexts": contexts})
        except Exception as e:
            self._json(500, {"error": str(e), "contexts": ["general"]})

    def _json(self, status: int, data: dict):
        body = json.dumps(data).encode("utf-8")
        self.send_response(status)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)

    def log_message(self, format, *args):
        pass


# ─── MAIN ──────────────────────────────────────────────────────────────
def main():
    monitor_thread = threading.Thread(target=monitor_loop, daemon=True)
    monitor_thread.start()

    server = HTTPServer((HOST, PORT), Handler)
    print(f"Context classifier running on http://{HOST}:{PORT}", flush=True)
    try:
        server.serve_forever()
    except KeyboardInterrupt:
        print("\nShutting down...", flush=True)
        server.shutdown()


if __name__ == "__main__":
    main()