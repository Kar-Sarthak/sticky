"""
Sticky Notes Context Classifier & Window Monitor Server
--------------------------------------------------------
1. HTTP server that classifies todo text into app/website contexts
   using Google's Gemini API.
2. Background window monitor that detects the active window title
   and browser URL (for Firefox/Chrome/Edge).

Endpoints:
  POST /classify  - {"text": "task description"} → {"contexts": ["app1", "app2"]}
  GET  /health    - {"status": "ok"}
"""

import json
import sys
import os
import threading
import time
from http.server import HTTPServer, BaseHTTPRequestHandler
from dotenv import load_dotenv

# ─── LOAD .ENV ─────────────────────────────────────────────────────────
load_dotenv(os.path.join(os.path.dirname(os.path.abspath(__file__)), ".env"))

# ─── CONFIG ────────────────────────────────────────────────────────────
HOST = "127.0.0.1"
PORT = 8765
API_KEY = os.environ.get("GEMINI_API_KEY", "")
MODEL = "gemini-3.1-flash-lite"
POLL_INTERVAL = 1  # seconds between window checks

# ─── GEMINI CLIENT ─────────────────────────────────────────────────────
try:
    from google import genai
except ImportError:
    print("ERROR: google-genai not installed. Run: pip install google-genai", file=sys.stderr)
    sys.exit(1)

if not API_KEY:
    print("ERROR: GEMINI_API_KEY not set. Add it to src-tauri/.env", file=sys.stderr)
    sys.exit(1)

client = genai.Client(api_key=API_KEY)

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

    # Process-based contexts
    for proc_key, ctx_name in CONTEXT_PROCESS_MAP.items():
        if proc_key in process:
            contexts.add(ctx_name)
            break

    return list(contexts), url


# ─── TODO MATCHING ─────────────────────────────────────────────────────

# Resolve paths relative to the app data dir where contexts.json/todos.json live
def _get_app_data_dir():
    """Find the Tauri app data directory where store files live."""
    import pathlib
    # Windows: %APPDATA%\com.sticky-notes.app\
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
        # Check if any of this todo's contexts match active contexts
        for tc in todo_ctx:
            tc_lower = tc.lower()
            if any(tc_lower in ac or ac in tc_lower for ac in active_lower):
                # Attach contexts to the todo for display
                todo_with_ctx = dict(todo)
                todo_with_ctx["contexts"] = todo_ctx
                matched.append(todo_with_ctx)
                break

    return matched


def monitor_loop():
    """Background thread that polls the active window and shows todo reminders."""
    if not HAS_WINDOW_DEPS:
        return

    # Initialize COM for this background thread (uiautomation requires it)
    import comtypes
    comtypes.CoInitialize()

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

            # Detect context (URL + process mapping)
            active_contexts, url = detect_current_contexts(title, process, hwnd)

            # Check if anything changed
            title_changed = title != last_title
            url_changed = url != last_url
            process_changed = process != last_process
            context_changed = set(active_contexts) != set(last_contexts)

            # Find matching todos
            matched_todos = get_todos_for_contexts(active_contexts) if active_contexts else []
            current_todo_ids = [t["id"] for t in matched_todos]
            todos_changed = current_todo_ids != last_todo_ids

            # Print window info if something changed
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
                last_contexts = active_contexts
                last_todo_ids = current_todo_ids
            elif todos_changed and matched_todos:
                # Same context but todos changed
                print(f"  📋 Todo list updated for {', '.join(active_contexts)}:", flush=True)
                for todo in matched_todos:
                    done = "✅" if todo.get("status") == "done" else "☐"
                    task = todo.get("task", "??")
                    print(f"     {done} {task}", flush=True)
                print()
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
- Include up to 3 contexts max if multiple are strongly relevant
- If uncertain, return ["general"]
- Do NOT add markdown, text, or reasoning
- Output must be valid JSON

Task: {text}
"""


def classify_todo(text: str) -> list[str]:
    prompt = CLASSIFY_PROMPT.format(text=text)
    response = client.models.generate_content(model=MODEL, contents=prompt)
    try:
        data = json.loads(response.text)
        return data.get("contexts", ["general"])
    except Exception:
        cleaned = response.text.strip().replace("```json", "").replace("```", "").strip()
        try:
            data = json.loads(cleaned)
            return data.get("contexts", ["general"])
        except Exception:
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
        # Suppress default stderr logging
        pass


# ─── MAIN ──────────────────────────────────────────────────────────────
def main():
    # Start window monitor in a background daemon thread
    monitor_thread = threading.Thread(target=monitor_loop, daemon=True)
    monitor_thread.start()

    # Start HTTP server
    server = HTTPServer((HOST, PORT), Handler)
    print(f"Context classifier running on http://{HOST}:{PORT}", flush=True)
    try:
        server.serve_forever()
    except KeyboardInterrupt:
        print("\nShutting down...", flush=True)
        server.shutdown()


if __name__ == "__main__":
    main()
