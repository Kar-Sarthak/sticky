import time
import json
import re
import win32gui
import win32process
import psutil
import uiautomation as auto

TODO_FILE = "todos.json"
POLL_INTERVAL = 1  # seconds

# ----------------------------
# CONTEXT → DOMAIN MAPPING
# Maps known context names to URL keywords to look for
# ----------------------------
CONTEXT_URL_MAP = {
    "linkedin":     ["linkedin.com"],
    "github":       ["github.com"],
    "gmail":        ["mail.google.com", "gmail.com"],
    "youtube":      ["youtube.com", "youtu.be"],
    "leetcode":     ["leetcode.com"],
    "notion":       ["notion.so", "notion.site"],
    "chatgpt":      ["chatgpt.com", "chat.openai.com"],
    "google docs":  ["docs.google.com/document"],
    "google sheets":["docs.google.com/spreadsheets"],
    "google slides":["docs.google.com/presentation"],
    "twitter":      ["twitter.com", "x.com"],
    "instagram":    ["instagram.com"],
    "reddit":       ["reddit.com"],
    "stackoverflow":["stackoverflow.com"],
    "trello":       ["trello.com"],
    "jira":         ["atlassian.net", "jira.com"],
    "figma":        ["figma.com"],
    "vercel":       ["vercel.com"],
    "netlify":      ["netlify.com"],
    "heroku":       ["heroku.com"],
    "aws":          ["aws.amazon.com", "console.aws.amazon.com"],
}

# Process name → context name mapping
CONTEXT_PROCESS_MAP = {
    "code.exe":         "vscode",
    "code":             "vscode",
    "slack.exe":        "slack",
    "slack":            "slack",
    "discord.exe":      "discord",
    "discord":          "discord",
    "zoom.exe":         "zoom",
    "zoom":             "zoom",
    "teams.exe":        "microsoft teams",
    "outlook.exe":      "outlook",
    "spotify.exe":      "spotify",
    "obsidian.exe":     "obsidian",
    "notion.exe":       "notion",
    "figma.exe":        "figma",
    "postman.exe":      "postman",
    "pycharm64.exe":    "pycharm",
    "idea64.exe":       "intellij",
    "webstorm64.exe":   "webstorm",
    "androidstudio64.exe": "android studio",
}

# ----------------------------
# WINDOW DETECTION
# ----------------------------
def get_active_window():
    hwnd = win32gui.GetForegroundWindow()
    if not hwnd:
        return None, None, None

    window_title = win32gui.GetWindowText(hwnd)
    _, pid = win32process.GetWindowThreadProcessId(hwnd)

    try:
        process = psutil.Process(pid)
        process_name = process.name().lower()
    except (psutil.NoSuchProcess, psutil.AccessDenied):
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
        # Chrome address bar
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

# ----------------------------
# CONTEXT DETECTION
# ----------------------------
def detect_current_contexts(title, process, hwnd):
    """
    Returns a list of detected context strings for the current window.
    """
    contexts = set()

    # Check URL-based contexts (for browsers)
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

    # Check process-based contexts
    for proc_key, ctx_name in CONTEXT_PROCESS_MAP.items():
        if proc_key in process:
            contexts.add(ctx_name)
            break

    return list(contexts), url

# ----------------------------
# TODO LOADER
# ----------------------------
def load_todos():
    try:
        with open(TODO_FILE, "r") as f:
            return json.load(f)
    except (FileNotFoundError, json.JSONDecodeError):
        return []

def get_todos_for_contexts(active_contexts):
    """Return todos whose contexts overlap with active_contexts."""
    todos = load_todos()
    matched = []
    active_lower = [c.lower() for c in active_contexts]
    for todo in todos:
        todo_contexts = [c.lower() for c in todo.get("contexts", [])]
        for tc in todo_contexts:
            if any(tc in ac or ac in tc for ac in active_lower):
                matched.append(todo)
                break
    return matched

# ----------------------------
# DISPLAY
# ----------------------------
def print_banner():
    print("=" * 55)
    print("  🔍 Sticky Todo Monitor — Context-Aware Reminders")
    print("=" * 55)
    print("  Watching your active window... (Ctrl+C to stop)\n")

def print_reminder(contexts, todos, url=None):
    ctx_str = ", ".join(contexts)
    print(f"\n{'─'*55}")
    print(f"  📍 Context detected: [{ctx_str}]")
    if url:
        short_url = url[:60] + "..." if len(url) > 60 else url
        print(f"  🌐 URL: {short_url}")
    print(f"  📋 Your todos for this context:")
    for todo in todos:
        ctx_tags = ", ".join(todo["contexts"])
        print(f"     • {todo['text']}  [{ctx_tags}]")
    print(f"{'─'*55}\n")

# ----------------------------
# MAIN MONITOR LOOP
# ----------------------------
def main():
    print_banner()

    last_contexts = []
    last_url = None
    last_todos_shown = []

    while True:
        title, process, hwnd = get_active_window()

        if process is None:
            time.sleep(POLL_INTERVAL)
            continue

        active_contexts, current_url = detect_current_contexts(title, process, hwnd)

        if not active_contexts:
            if last_contexts:
                # Switched away from a known context
                last_contexts = []
                last_url = None
                last_todos_shown = []
            time.sleep(POLL_INTERVAL)
            continue

        matched_todos = get_todos_for_contexts(active_contexts)

        # Only print if context changed or URL changed significantly or todos changed
        context_changed = set(active_contexts) != set(last_contexts)
        url_changed = current_url != last_url
        todos_changed = [t["id"] for t in matched_todos] != [t["id"] for t in last_todos_shown]

        if (context_changed or url_changed or todos_changed) and matched_todos:
            print_reminder(active_contexts, matched_todos, current_url)

        last_contexts = active_contexts
        last_url = current_url
        last_todos_shown = matched_todos

        time.sleep(POLL_INTERVAL)

if __name__ == "__main__":
    try:
        main()
    except KeyboardInterrupt:
        print("\n\n👋 Monitor stopped. Goodbye!")
