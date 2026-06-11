# Sticky Notes - Comprehensive Packaging & Build Guide

This guide details every step, edit, and configuration change required to take your base Sticky Notes application and compile it into a production-ready, standalone Windows installer (`setup.exe`).

By following these instructions, you can reproduce the exact working build from scratch.

---

## Phase 1: Python Sidecar Preparation

The background Python monitor and context server needs to run completely invisibly (headless) on Windows. If it tries to output standard messages (like [print()](file:///e:/sticky%20build/todo_monitor.py#175-180)) to a non-existent terminal, it will crash. 

### 1. Update [todo_context_server.py](file:///e:/sticky%20build/src-tauri/todo_context_server.py)

Make the following critical edits to [todo_context_server.py](file:///e:/sticky%20build/src-tauri/todo_context_server.py):

**A. Add Core Imports and Safe Logging**
At the top of the file, ensure all dependencies are imported and redirect the standard streams to prevent `OSError` crashes:

```python
import os
import json
import re
import sys
import threading
import time
import urllib.request
from http.server import HTTPServer, BaseHTTPRequestHandler

# Hardcode API Key to avoid `.env` dependencies in production
GROQ_API_KEY = "your_actual_api_key_here"
GROQ_URL = "https://api.groq.com/openai/v1/chat/completions"

# ─── SAFE LOGGING ──────────────────────────────────────────────────────
def log(msg, error=False):
    """Safe logging that won't crash if stdout/stderr are missing (Windows)."""
    try:
        stream = sys.stderr if error else sys.stdout
        if stream and not stream.closed:
            print(msg, file=stream, flush=True)
    except OSError:
        pass

# Redirect stdout/stderr to devnull if they are missing
if sys.stdout is None: sys.stdout = open(os.devnull, "w")
if sys.stderr is None: sys.stderr = open(os.devnull, "w")

# Force UTF-8 encoding for standard streams to prevent Unicode/Emoji crashes
try:
    if hasattr(sys.stdout, "reconfigure"):
        sys.stdout.reconfigure(encoding='utf-8')
    if hasattr(sys.stderr, "reconfigure"):
        sys.stderr.reconfigure(encoding='utf-8')
except Exception:
    pass
```

**B. Replace ALL [print()](file:///e:/sticky%20build/todo_monitor.py#175-180) calls with [log()](file:///e:/sticky%20build/src-tauri/todo_context_server.py#34-42)**
Find and replace every single [print(f"...")](file:///e:/sticky%20build/todo_monitor.py#175-180) in the file with [log(f"...")](file:///e:/sticky%20build/src-tauri/todo_context_server.py#34-42). This stops the app from crashing when processing Unicode output (like emojis).

**C. Robust App Data Resolution**
Make sure the background monitor can reliably find your `todos.json` on any Windows PC:

```python
def _get_app_data_dir():
    """Find the Tauri app data directory where store files live."""
    appdata = os.environ.get("APPDATA")
    if not appdata:
        return None
        
    # Try common folder names for this app
    possible_folders = ["com.sticky-notes.app", "sticky-notes"]
    for folder in possible_folders:
        store_dir = os.path.join(appdata, folder)
        if os.path.exists(store_dir):
            return store_dir
            
    return None
```

**D. Dynamic Slash Command Matching**
Read slash commands directly from the user's saved contexts so custom `/app` names work:

```python
def classify_todo(text: str) -> list[str]:
    # Match any slash command including hyphens
    match = re.search(r'(?:^|\s)/([a-zA-Z0-9_\-]+)', text)

    if match:
        cmd = match.group(1).lower()

        # A. Check known hardcoded commands
        if cmd in SLASH_COMMAND_MAP:
            context_name = SLASH_COMMAND_MAP[cmd]
            log(f"  [classify] ⚡ Slash command detected: /{cmd} -> {context_name}")
            return [context_name]

        # B. Check actual saved contexts from contexts.json
        saved_contexts = _get_all_saved_contexts()
        for ctx in saved_contexts:
            if ctx.lower() == cmd:
                log(f"  [classify] ⚡ Saved context matched: /{cmd} -> {ctx}")
                return [ctx]

        # C. Unknown command → save the raw word as context directly
        log(f"  [classify] ⚡ Unknown slash command: /{cmd} -> saving as-is")
        return [cmd]
        
    # ... fallback to GROQ AI ...
```

### 2. Compile the Python Sidecar

Run the following PyInstaller command inside your `src-tauri` directory to create a self-contained executable:

```bash
pyinstaller --onefile --noconsole --name todo_context_server-x86_64-pc-windows-msvc todo_context_server.py
```
After successful compilation, move the resulting [.exe](file:///e:/sticky%20build/src-tauri/bin/todo_context_server-x86_64-pc-windows-msvc.exe) out of the `dist` folder directly into the root of `src-tauri`.
```bash
move dist\todo_context_server-x86_64-pc-windows-msvc.exe .
```

---

## Phase 2: Tauri Configuration

Tauri needs strict definitions on what sidecars belong to the app and how to handle plugins.

### 1. [tauri.conf.json](file:///e:/sticky%20build/src-tauri/tauri.conf.json)

Add the `externalBin` array to register the python sidecar.
**CRITICAL FIX**: Change any empty plugin configurations from `{}` to `null`. Using `{}` causes application panics in Tauri v2!

```json
{
  "bundle": {
    "identifier": "com.sticky-notes.app",
    "externalBin": [
      "todo_context_server"
    ]
  },
  "plugins": {
    "store": null,
    "shell": null,
    "global-shortcut": null,
    "autostart": null 
  }
}
```

### 2. [capabilities/default.json](file:///e:/sticky%20build/src-tauri/capabilities/default.json)

Grant Tauri permission to spawn the Python sidecar.

```json
{
  "$schema": "../gen/schemas/desktop-schema.json",
  "identifier": "default",
  "description": "Capability for the main window",
  "windows": ["*"],
  "permissions": [
    "core:default",
    "shell:default",
    {
      "identifier": "shell:allow-spawn",
      "allow": [
        {
          "name": "todo_context_server"
        }
      ]
    }
  ]
}
```

---

## Phase 3: Rust Backend Integration

Connect the frontend, autostart functionality, and sidecar execution inside the Rust Backend.

### 1. [src/main.rs](file:///e:/sticky%20build/src-tauri/src/main.rs)

To ensure no black terminal window appears when users launch the app, include the `windows_subsystem` attribute on line 1:

```rust
// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    sticky_notes_lib::run()
}
```

### 2. [src/lib.rs](file:///e:/sticky%20build/src-tauri/src/lib.rs)

Make the following updates to orchestrate the sidecar and enable autostart logic.

**A. Import correct traits**
```rust
use tauri_plugin_autostart::{MacosLauncher, ManagerExt};
use tauri_plugin_shell::ShellExt;
```

**B. Refactor [spawn_context_server](file:///e:/sticky%20build/src-tauri/src/lib.rs#26-62)**
Use the `shell()` plugin API instead of standard library commands to correctly resolve the bundled sidecar path:
```rust
fn spawn_context_server(app: &tauri::AppHandle) {
    let sidecar_command = match app.shell().sidecar("todo_context_server") {
        Ok(cmd) => cmd,
        Err(e) => {
            eprintln!("[context] Failed to create sidecar command: {}", e);
            return;
        }
    };

    let (_, child) = match sidecar_command.spawn() {
        Ok(res) => res,
        Err(e) => {
            eprintln!("[context] Failed to spawn sidecar: {}", e);
            return;
        }
    };
    
    // Process is now running completely independent of the main app thread.
    println!("[context] sidecar started (pid: {:?})", child.pid());
}
```

**C. Initialize Plugins in [run()](file:///e:/sticky%20build/src-tauri/src/lib.rs#1525-1715)**
Within [run()](file:///e:/sticky%20build/src-tauri/src/lib.rs#1525-1715), ensure all necessary plugins are chained, especially `shell` and `autostart`, and call `autolaunch().enable()` during the `setup` phase:

```rust
#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_store::Builder::default().build())
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .plugin(tauri_plugin_autostart::init(MacosLauncher::LaunchAgent, Some(vec!["--autostart"])))
        .setup(|app| {
            // --- Enable autostart by default ---
            // Triggers Windows Registry update to start app on boot
            let _ = app.autolaunch().enable();

            // Spawn the Python context classifier server
            spawn_context_server(app.handle());
            
            // ... Initialize your stores and UI windows here ...
            
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
```

---

## Phase 4: Final Distribution Build

Once all file modifications are complete and your environment is ready, generate the final MSI/exe setup installer:

```bash
npm run tauri build
```

This compiles both the frontend (React/Vite) and the backend (Rust), bundles your Python executable alongside it, configures the registry logic, and packs it all into an easy-to-install `setup.exe` available inside `src-tauri/target/release/bundle/`.
