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


def monitor_loop():
    """Background thread that polls the active window and prints changes."""
    if not HAS_WINDOW_DEPS:
        return

    # Initialize COM for this background thread (uiautomation requires it)
    import comtypes
    comtypes.CoInitialize()

    last_title = None
    last_url = None
    last_process = None

    print("\n🔍 Window monitor active — watching active window...")

    while True:
        try:
            title, process, hwnd = get_active_window()

            if title is None or process is None:
                time.sleep(POLL_INTERVAL)
                continue

            # Try to get URL for browsers
            url = None
            if "firefox" in process:
                url = get_firefox_url(hwnd)
            elif "chrome" in process:
                url = get_chrome_url(hwnd)
            elif "msedge" in process or "edge" in process:
                url = get_edge_url(hwnd)

            # Only print when something changed
            title_changed = title != last_title
            url_changed = url != last_url
            process_changed = process != last_process

            if title_changed or url_changed or process_changed:
                display_url = f" | URL: {url[:80]}" if url else ""
                print(f"  🪟 [{process}] {title}{display_url}", flush=True)

                last_title = title
                last_url = url
                last_process = process

        except Exception as e:
            # Don't crash the monitor on a single bad poll
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
