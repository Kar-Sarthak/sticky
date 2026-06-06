# Sticky Notes App — Implementation Plan

**Tauri 2 · React + TypeScript · Rust**

> This document is the single source of truth for implementing the entire app.
> Each phase is self-contained: finish one phase, verify it works, then move to the next.
> Future sessions should read this file, implement the requested phase, and stop for confirmation.

---

## Table of Contents

1. [Tech Stack Decisions](#1-tech-stack-decisions)
2. [Project File Structure](#2-project-file-structure)
3. [Phase 1: Project Foundation](#phase-1-project-foundation)
4. [Phase 2: System Tray](#phase-2-system-tray)
5. [Phase 3: Global Hotkey & Note Window Spawning](#phase-3-global-hotkey--note-window-spawning)
6. [Phase 4: Click-Through Behavior](#phase-4-click-through-behavior)
7. [Phase 5: Note UI](#phase-5-note-ui)
8. [Phase 6: Add Note Button](#phase-6-add-note-button)
9. [Cross-Cutting Concerns](#cross-cutting-concerns)
10. [Known Gotchas & Edge Cases](#known-gotchas--edge-cases)

---

## 1. Tech Stack Decisions

| Layer       | Choice                            | Rationale                                        |
|-------------|------------------------------------|--------------------------------------------------|
| Backend     | Tauri 2 (Rust)                    | Native binary, separate WebviewWindow per note  |
| Frontend    | React 18 + TypeScript + Vite      | Largest ecosystem, simplest Tauri integration   |
| Styling     | Plain CSS (no framework)          | App is small; avoids bundle bloat               |
| Persistence | `tauri-plugin-store` (JSON file)  | Zero-config, built into Tauri ecosystem          |
| Tray/Hotkey | Tauri 2 core `tray-icon` + `global-shortcut` plugin | Native APIs, no extra deps |

### Key Dependencies

```toml
# Cargo.toml
tauri = { version = "2", features = ["tray-icon", "image-png"] }
tauri-plugin-store = "2"
tauri-plugin-global-shortcut = "2"
serde = { version = "1", features = ["derive"] }
serde_json = "1"
```

```json
// package.json (frontend)
{
  "react": "^18",
  "react-dom": "^18",
  "@tauri-apps/api": "^2",
  "@tauri-apps/plugin-store": "^2",
  "@tauri-apps/plugin-global-shortcut": "^2"
}
```

---

## 2. Project File Structure

```
sticky-notes/
├── src-tauri/
│   ├── Cargo.toml
│   ├── tauri.conf.json
│   ├── build.rs
│   ├── capabilities/
│   │   └── default.json          # Permissions for webview window creation, store, global-shortcut
│   ├── icons/                     # Tauri auto-generated or manual tray/app icons
│   └── src/
│       ├── main.rs               # Tauri app entry, setup hook, tray, hotkey registration
│       ├── lib.rs                # (Optional) split logic if main.rs grows
│       ├── models.rs             # Rust Note struct + serde
│       └── commands.rs           # Tauri commands: create_note, delete_note, etc.
│
├── src/                          # React frontend
│   ├── main.tsx                  # React entry point — detects window label, mounts correct component
│   ├── index.html
│   ├── styles/
│   │   ├── global.css            # App-wide resets, fonts
│   │   └── note.css              # Note window styles (header, body, drag region)
│   ├── components/
│   │   ├── NoteWindow.tsx        # Rendered inside each note window
│   │   ├── PreferencesWindow.tsx # Rendered inside preferences window
│   │   └── AddButtonWindow.tsx   # Rendered inside the floating + button window
│   └── utils/
│       ├── store.ts              # Store helper wrapper (get/set notes array)
│       ├── noteFactory.ts        # ID generation, default note creation
│       └── spawnWindow.ts        # Frontend function: WebviewWindow.create() with note config
│
├── package.json
├── tsconfig.json
├── vite.config.ts
└── IMPLEMENTATION_PLAN.md        # This file
```

---

## Phase 1: Project Foundation

**Goal:** Scaffold the full project, define the data model, wire persistence, configure background-only lifecycle.

### Steps

#### 1.1 — Scaffold Tauri 2 + React project

Run from the `e:\sticky` directory:

```bash
# Install create-tauri-app globally or use npx
npm create tauri-app@latest . -- --template react-ts
```

- When prompted for package manager, choose `npm`
- After scaffolding, the directory will contain `src-tauri/` and `src/`

#### 1.2 — Install plugins

```bash
cd src-tauri
cargo add tauri-plugin-store
cargo add tauri-plugin-global-shortcut
```

#### 1.3 — Define the note data model

**Rust side** — `src-tauri/src/models.rs`:

```rust
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Note {
    pub id: String,
    pub title: String,
    pub content: String,
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    pub color: String,    // hex color, e.g. "#FFF9C4"
}
```

**TypeScript side** — `src/types.ts` (create this file):

```ts
export interface Note {
  id: string;
  title: string;
  content: string;
  x: number;
  y: number;
  width: number;
  height: number;
  color: string;
}
```

#### 1.4 — Configure `tauri.conf.json`

Key settings to set in the generated `tauri.conf.json`:

```json
{
  "productName": "sticky-notes",
  "version": "0.1.0",
  "identifier": "com.sticky-notes.app",
  "app": {
    "withGlobalTauri": true,
    "windows": []
  },
  "build": {
    "beforeDevCommand": "npm run dev",
    "devUrl": "http://localhost:1420",
    "frontendDist": "../dist"
  },
  "bundle": {
    "active": true,
    "targets": "all",
    "icon": [
      "icons/32x32.png",
      "icons/128x128.png",
      "icons/128x128@2x.png",
      "icons/icon.icns",
      "icons/icon.ico"
    ]
  }
}
```

- **`"app": { "windows": [] }`** — No main window on startup. The app runs in the background only.

#### 1.5 — Configure capabilities

Create `src-tauri/capabilities/default.json`:

```json
{
  "identifier": "default",
  "windows": ["*"],
  "permissions": [
    "core:default",
    "core:webview:allow-create-webview-window",
    "core:window:allow-set-ignore-cursor-events",
    "core:window:allow-start-dragging",
    "core:window:allow-close",
    "core:window:allow-destroy",
    "core:window:allow-set-position",
    "core:window:allow-set-size",
    "core:window:allow-set-always-on-top",
    "core:window:allow-current-monitor",
    "core:window:allow-inner-position",
    "core:window:allow-inner-size",
    "core:window:allow-on-moved",
    "core:window:allow-on-resized",
    "core:event:allow-emit",
    "core:event:allow-listen",
    "store:default",
    "global-shortcut:default"
  ]
}
```

#### 1.6 — Persistence layer (store)

**Rust side** — Initialize the store plugin in `src-tauri/src/main.rs`:

```rust
fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_store::Builder::default().build())
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .setup(|app| {
            // Initialize store file
            let store = app.store("notes.json")?;
            // Ensure notes array exists
            if store.get("notes").is_none() {
                store.set("notes", serde_json::json!([]));
                store.save()?;
            }
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri");
}
```

**TypeScript side** — `src/utils/store.ts`:

```ts
import { Store } from '@tauri-apps/plugin-store';

let store: Store | null = null;

export async function getStore(): Promise<Store> {
    if (!store) {
        store = await Store.load('notes.json');
    }
    return store;
}

export async function getNotes(): Promise<any[]> {
    const s = await getStore();
    return (await s.get<any[]>('notes')) || [];
}

export async function saveNotes(notes: any[]): Promise<void> {
    const s = await getStore();
    await s.set('notes', notes);
    await s.save();
}

export async function addNote(note: any): Promise<void> {
    const notes = await getNotes();
    notes.push(note);
    await saveNotes(notes);
}

export async function updateNote(id: string, updates: Partial<any>): Promise<void> {
    const notes = await getNotes();
    const idx = notes.findIndex((n: any) => n.id === id);
    if (idx !== -1) {
        notes[idx] = { ...notes[idx], ...updates };
        await saveNotes(notes);
    }
}

export async function deleteNote(id: string): Promise<void> {
    const notes = await getNotes();
    await saveNotes(notes.filter((n: any) => n.id !== id));
}
```

#### 1.7 — Window close → hide, not quit

In `src-tauri/src/main.rs`, inside `.setup()`, register a listener:

```rust
use tauri::Manager;

// Inside setup():
app.on_window_event(|window, event| {
    if let tauri::WindowEvent::CloseRequested { api, .. } = event {
        api.prevent_close();
        window.hide().ok();
    }
});
```

This ensures closing any window hides it instead of terminating the app process.

### Verification Checklist

- [ ] `npm run tauri dev` starts the app with no visible window
- [ ] A `notes.json` store file is created in the app data directory
- [ ] The app process stays running after closing windows (verify in task manager)
- [ ] `Note` struct compiles in Rust, `Note` interface exists in TypeScript

---

## Phase 2: System Tray

**Goal:** System tray icon with menu ("Show Notes", "Preferences", "Quit"), Preferences window with hotkey input.

### Steps

#### 2.1 — Generate a tray icon

- Create a simple 32x32 PNG icon at `src-tauri/icons/tray-icon.png`
- Can be a simple sticky-note shaped icon; use any image editor or Tauri's icon generator

#### 2.2 — Tray icon & menu in Rust

In `src-tauri/src/main.rs`, inside `.setup()`:

```rust
use tauri::{
    tray::{MouseButton, TrayIconBuilder, TrayIconEvent},
    Manager,
};

TrayIconBuilder::new()
    .icon(app.default_window_icon().unwrap().clone())
    .tooltip("Sticky Notes")
    .on_tray_icon_event(|tray, event| {
        if let TrayIconEvent::Click { button: MouseButton::Left, .. } = event {
            // Toggle visibility of all note windows
            let app = tray.app_handle();
            for window in app.webview_windows().values() {
                let label = window.label();
                if label.starts_with("note-") || label == "add-button" {
                    if window.is_visible().unwrap_or(false) {
                        window.hide().ok();
                    } else {
                        window.show().ok();
                        window.set_focus().ok();
                    }
                }
            }
        }
    })
    .menu(|app| {
        use tauri::menu::{Menu, MenuItem};
        Menu::new(app)?
    })
    .build(app)?;
```

**Menu items to add:**

```rust
.menu(|app| {
    use tauri::menu::{Menu, MenuItemBuilder, PredefinedMenuItem};
    let show_notes = MenuItemBuilder::with_id("show_notes", "Show Notes").build(app)?;
    let preferences = MenuItemBuilder::with_id("preferences", "Preferences").build(app)?;
    let separator = PredefinedMenuItem::separator(app)?;
    let quit = PredefinedMenuItem::quit(app, None)?;
    Menu::with_items(app, &[&show_notes, &preferences, &separator, &quit])
})
```

#### 2.3 — Handle tray menu clicks

```rust
.on_menu_event(|app, event| {
    match event.id().as_ref() {
        "show_notes" => {
            // Show all note windows
            for window in app.webview_windows().values() {
                let label = window.label();
                if label.starts_with("note-") || label == "add-button" {
                    window.show().ok();
                    window.set_focus().ok();
                }
            }
        }
        "preferences" => {
            // Spawn preferences window (see 2.4)
            if let Some(win) = app.get_webview_window("preferences") {
                win.show().ok();
                win.set_focus().ok();
            } else {
                // create it (handled in frontend or via command)
            }
        }
        "quit" => {
            app.exit(0);
        }
        _ => {}
    }
})
```

#### 2.4 — Preferences window

**Approach:** The frontend detects when it's loaded inside the preferences window and renders the preferences UI.

In `src-tauri/src/main.rs`, add a Tauri command to spawn the preferences window:

```rust
#[tauri::command]
async fn spawn_preferences_window(app: tauri::AppHandle) -> Result<(), String> {
    if let Some(win) = app.get_webview_window("preferences") {
        win.show().map_err(|e| e.to_string())?;
        win.set_focus().map_err(|e| e.to_string())?;
        return Ok(());
    }

    let url = "index.html#preferences";
    tauri::WebviewWindowBuilder::new(
        &app,
        "preferences",
        tauri::WebviewUrl::App(url.into()),
    )
    .title("Preferences")
    .inner_size(400.0, 300.0)
    .resizable(false)
    .build()
    .map_err(|e| e.to_string())?;

    Ok(())
}
```

Call this from the tray menu `"preferences"` handler instead of the TODO comment.

#### 2.5 — Preferences window UI

`src/components/PreferencesWindow.tsx`:

- A small window with:
  - **Hotkey input field** — displays current shortcut, lets user change it
  - **Save button** — writes the new hotkey to the store
- The hotkey preference is stored in the same `notes.json` store under key `"hotkey"`, default `"CommandOrControl+Shift+S"`

```tsx
// Sketch:
import { useState, useEffect } from 'react';
import { getStore } from '../utils/store';

export default function PreferencesWindow() {
    const [hotkey, setHotkey] = useState('CommandOrControl+Shift+S');

    useEffect(() => {
        getStore().then(s => s.get<string>('hotkey')).then(v => v && setHotkey(v));
    }, []);

    const handleSave = async () => {
        const s = await getStore();
        await s.set('hotkey', hotkey);
        await s.save();
        // Emit event to tell Rust to re-register
        // (Phase 3 will handle this)
    };

    return (
        <div className="preferences">
            <h2>Preferences</h2>
            <label>
                Global Shortcut:
                <input value={hotkey} onChange={e => setHotkey(e.target.value)} />
            </label>
            <button onClick={handleSave}>Save</button>
        </div>
    );
}
```

#### 2.6 — Frontend router: mount the right component

`src/main.tsx` — detect the window label/hash and render the correct React component:

```tsx
import React from 'react';
import ReactDOM from 'react-dom/client';
import { getCurrentWindow } from '@tauri-apps/api/window';
import NoteWindow from './components/NoteWindow';
import PreferencesWindow from './components/PreferencesWindow';
import AddButtonWindow from './components/AddButtonWindow';
import './styles/global.css';

const currentWindow = getCurrentWindow();
const label = currentWindow.label;
const hash = window.location.hash;

let App: React.ComponentType;

if (label.startsWith('note-')) {
    App = NoteWindow;
} else if (label === 'preferences' || hash === '#preferences') {
    App = PreferencesWindow;
} else if (label === 'add-button' || hash === '#add-button') {
    App = AddButtonWindow;
} else {
    // Should never happen since no main window
    App = () => <div>No UI</div>;
}

ReactDOM.createRoot(document.getElementById('root')!).render(<App />);
```

### Verification Checklist

- [ ] Tray icon appears in system tray on startup
- [ ] Right-click menu shows "Show Notes", "Preferences", "Quit"
- [ ] Clicking "Preferences" opens a small 400x300 window
- [ ] Preferences window has a hotkey input and save button
- [ ] Clicking "Quit" exits the app completely
- [ ] Left-click on tray toggles note window visibility

---

## Phase 3: Global Hotkey & Note Window Spawning

**Goal:** Register a global shortcut to toggle note visibility; spawn individual note windows; restore notes on launch.

### Steps

#### 3.1 — Global shortcut registration in Rust

In `src-tauri/src/main.rs`, inside `.setup()`:

```rust
use tauri_plugin_global_shortcut::{
    Code, GlobalShortcutExt, Modifiers, Shortcut, ShortcutState,
};

let default_hotkey = Shortcut::new(Some(Modifiers::CONTROL | Modifiers::SHIFT), Code::KeyS);

// Read custom hotkey from store, or use default
let hotkey = {
    let store = app.store("notes.json")?;
    store.get("hotkey")
        .and_then(|v| v.as_str())
        .and_then(|s| parse_hotkey_string(s).ok())
        .unwrap_or(default_hotkey)
};

app.global_shortcut().on_shortcut(hotkey, |app, _shortcut, event| {
    if event.state == ShortcutState::Pressed {
        // Toggle visibility
        let mut all_visible = true;
        for window in app.webview_windows().values() {
            let label = window.label();
            if label.starts_with("note-") || label == "add-button" {
                if window.is_visible().unwrap_or(false) == false {
                    all_visible = false;
                    break;
                }
            }
        }

        for window in app.webview_windows().values() {
            let label = window.label();
            if label.starts_with("note-") || label == "add-button" {
                if all_visible {
                    window.hide().ok();
                } else {
                    window.show().ok();
                    window.set_focus().ok();
                }
            }
        }
    }
}).expect("Failed to register global shortcut");
```

Add a helper function `parse_hotkey_string` to convert `"CommandOrControl+Shift+S"` into a `Shortcut`.

#### 3.2 — Re-register shortcut on preference change

Add a Tauri command `re_register_shortcut(new_hotkey: String)` that:
1. Unregisters the current shortcut
2. Parses the new hotkey string
3. Registers the new shortcut
4. Stores the new shortcut in the store

```rust
#[tauri::command]
async fn re_register_shortcut(
    app: tauri::AppHandle,
    new_hotkey: String,
) -> Result<(), String> {
    // Implementation: unregister old, register new
    // Store in notes.json under "hotkey" key
    Ok(())
}
```

#### 3.3 — Note window factory function (TypeScript)

`src/utils/spawnWindow.ts`:

```ts
import { WebviewWindow } from '@tauri-apps/api/webviewWindow';
import type { Note } from '../types';

export async function spawnNoteWindow(note: Note): Promise<WebviewWindow | null> {
    const label = `note-${note.id}`;

    // Check if already exists
    const existing = WebviewWindow.getByLabel(label);
    if (existing) {
        existing.show();
        existing.setFocus();
        return existing;
    }

    const webview = new WebviewWindow(label, {
        url: 'index.html',
        title: note.title || 'Note',
        x: Math.round(note.x),
        y: Math.round(note.y),
        width: Math.round(note.width),
        height: Math.round(note.height),
        resizable: true,
        decorations: false,
        transparent: true,
        alwaysOnTop: true,
        shadow: true,
    });

    return webview;
}

export async function spawnAddButtonWindow(): Promise<WebviewWindow | null> {
    const label = 'add-button';
    const existing = WebviewWindow.getByLabel(label);
    if (existing) {
        existing.show();
        existing.setFocus();
        return existing;
    }

    const { getCurrentMonitor } = await import('@tauri-apps/api/window');
    const monitor = await getCurrentMonitor();
    const { width: screenWidth, height: screenHeight } = monitor?.size || { width: 1920, height: 1080 };
    const { scaleFactor } = monitor || { scaleFactor: 1 };

    const webview = new WebviewWindow(label, {
        url: 'index.html#add-button',
        title: 'Add Note',
        x: Math.round(screenWidth - 80 * scaleFactor),
        y: Math.round(screenHeight - 80 * scaleFactor),
        width: 60,
        height: 60,
        resizable: false,
        decorations: false,
        transparent: true,
        alwaysOnTop: true,
        skipTaskbar: true,
    });

    return webview;
}
```

#### 3.4 — Restore notes on launch

In `src/main.tsx`, after mounting the correct component, if the window label is empty (i.e., this is the hidden startup context), trigger restoration:

**Better approach:** Do this from the Rust side in `.setup()` after the store is initialized:

```rust
// In main.rs setup(), after initializing the store:
// Emit an event that the frontend listens to, telling it to spawn windows
// OR spawn windows directly from Rust
```

**Recommended approach — spawn from Rust for reliability:**

```rust
// In setup(), after store init:
let notes = store.get("notes").unwrap();
if let Some(notes_arr) = notes.as_array() {
    for note in notes_arr {
        let id = note.get("id").and_then(|v| v.as_str()).unwrap_or("");
        let title = note.get("title").and_then(|v| v.as_str()).unwrap_or("Note");
        let x = note.get("x").and_then(|v| v.as_f64()).unwrap_or(100.0);
        let y = note.get("y").and_then(|v| v.as_f64()).unwrap_or(100.0);
        let width = note.get("width").and_then(|v| v.as_f64()).unwrap_or(300.0);
        let height = note.get("height").and_then(|v| v.as_f64()).unwrap_or(200.0);

        let url = format!("index.html#note-{}", id);
        let label = format!("note-{}", id);

        tauri::WebviewWindowBuilder::new(
            app,
            &label,
            tauri::WebviewUrl::App(url.into()),
        )
        .title(title)
        .inner_size(width, height)
        .position(x, y)
        .resizable(true)
        .decorations(false)
        .transparent(true)
        .always_on_top(true)
        .build()
        .ok();
    }
}
```

Then update `src/main.tsx` to also check `window.location.hash` for `#note-{id}` pattern to detect note windows spawned from Rust.

#### 3.5 — Create Note command (Tauri command)

```rust
#[tauri::command]
async fn create_note(app: tauri::AppHandle) -> Result<Note, String> {
    let store = app.store("notes.json").map_err(|e| e.to_string())?;
    let mut notes: Vec<Note> = store
        .get("notes")
        .and_then(|v| serde_json::from_value(v.clone()).ok())
        .unwrap_or_default();

    let id = uuid::Uuid::new_v4().to_string();
    // Offset position from last note
    let offset = notes.len() * 30;
    let note = Note {
        id,
        title: "ToDo".to_string(),
        content: String::new(),
        x: 100.0 + offset as f64,
        y: 100.0 + offset as f64,
        width: 300.0,
        height: 200.0,
        color: "#FFF9C4".to_string(),
    };

    notes.push(note.clone());
    store.set("notes", serde_json::to_value(&notes).unwrap());
    store.save().map_err(|e| e.to_string())?;

    // Spawn the window for this note
    // ... same WebviewWindowBuilder code as in 3.4

    Ok(note)
}
```

### Verification Checklist

- [ ] Pressing `Ctrl+Shift+S` toggles all note windows show/hide
- [ ] Changing the hotkey in Preferences and saving re-registers the shortcut
- [ ] On app launch, previously saved notes reappear in their saved positions
- [ ] Calling `create_note` from the frontend creates a new note window and persists it

---

## Phase 4: Click-Through Behavior

**Goal:** Notes are transparent — clicking the background clicks the desktop beneath. Only UI elements (header, textarea) capture input.

### Steps

#### 4.1 — mousemove toggle pattern

In `src/components/NoteWindow.tsx`, add a `mousemove` handler on the root element:

```tsx
import { getCurrentWindow } from '@tauri-apps/api/window';

function NoteWindow() {
    const windowRef = useRef<HTMLDivElement>(null);

    useEffect(() => {
        const el = windowRef.current;
        if (!el) return;

        const handleMouseMove = (e: MouseEvent) => {
            const target = document.elementFromPoint(e.clientX, e.clientY);
            const ignoreEvents = target === el || target === el.querySelector('.note-background');
            getCurrentWindow().setIgnoreCursorEvents(ignoreEvents);
        };

        el.addEventListener('mousemove', handleMouseMove);
        return () => {
            el.removeEventListener('mousemove', handleMouseMove);
            getCurrentWindow().setIgnoreCursorEvents(false);
        };
    }, []);

    return (
        <div ref={windowRef} className="note-window" style={{ background: 'transparent' }}>
            {/* header and body */}
        </div>
    );
}
```

#### 4.2 — Disable click-through during interaction

- On `focus` of the textarea or title input: `setIgnoreCursorEvents(false)`
- On `blur` of the textarea/title: re-evaluate via the mousemove handler
- During drag operations (Tauri drag region is handled natively, no extra code needed)
- On `mousedown` anywhere on the note: `setIgnoreCursorEvents(false)`

```tsx
// Add to NoteWindow.tsx:
const ensureInput = () => {
    getCurrentWindow().setIgnoreCursorEvents(false);
};

// Attach to textarea onFocus, onInput, onDrag
// Attach to title span onFocus
// Attach to root div onMouseDown
```

#### 4.3 — Apply click-through on window load

```tsx
useEffect(() => {
    // Start with ignore enabled (transparent background)
    getCurrentWindow().setIgnoreCursorEvents(true);
}, []);
```

### Verification Checklist

- [ ] Clicking on the transparent part of a note window clicks through to the desktop/app beneath
- [ ] Clicking on the header bar or textarea properly captures input
- [ ] Typing in the textarea never loses focus to click-through
- [ ] Dragging the window by the header works normally

---

## Phase 5: Note UI

**Goal:** Complete note appearance and interaction — header bar, action buttons, editable title, auto-saving textarea, position sync.

### Steps

#### 5.1 — Custom header bar

`src/components/NoteWindow.tsx`:

```tsx
<header className="note-header" data-tauri-drag-region>
    <span
        className="note-title"
        contentEditable
        suppressContentEditableWarning
        onBlur={handleTitleSave}
    >
        {note.title || 'ToDo'}
    </span>
    <div className="note-actions">
        <button className="btn-close" onClick={handleClose} title="Hide">✕</button>
        <button className="btn-delete" onClick={handleDelete} title="Delete">🗑</button>
    </div>
</header>
```

`src/styles/note.css`:

```css
.note-window {
    width: 100%;
    height: 100%;
    display: flex;
    flex-direction: column;
    border-radius: 8px;
    overflow: hidden;
    box-shadow: 0 4px 20px rgba(0, 0, 0, 0.15);
}

.note-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 8px 12px;
    cursor: grab;
    user-select: none;
    background: rgba(0, 0, 0, 0.05);
    border-bottom: 1px solid rgba(0, 0, 0, 0.08);
}

.note-title {
    font-size: 14px;
    font-weight: 600;
    flex: 1;
    outline: none;
    cursor: text;
}

.note-actions {
    display: flex;
    gap: 4px;
    opacity: 0;
    transition: opacity 0.15s ease;
}

.note-header:hover .note-actions {
    opacity: 1;
}

.note-actions button {
    background: none;
    border: none;
    cursor: pointer;
    font-size: 14px;
    padding: 2px 4px;
    border-radius: 4px;
    opacity: 0.6;
}

.note-actions button:hover {
    opacity: 1;
    background: rgba(0, 0, 0, 0.1);
}
```

#### 5.2 — Editable title — auto-save on blur

```tsx
const [title, setTitle] = useState(note.title || 'ToDo');

const handleTitleSave = (e: React.FocusEvent<HTMLSpanElement>) => {
    const newTitle = e.currentTarget.textContent?.trim() || 'ToDo';
    setTitle(newTitle);
    updateNote(note.id, { title: newTitle });
};
```

#### 5.3 — Note body textarea — auto-save with debounce

```tsx
import { useCallback, useRef } from 'react';

const handleContentChange = useCallback(
    debounce((value: string) => {
        updateNote(note.id, { content: value });
    }, 300),
    [note.id]
);

function debounce(fn: Function, ms: number) {
    let timer: ReturnType<typeof setTimeout>;
    return (...args: any[]) => {
        clearTimeout(timer);
        timer = setTimeout(() => fn(...args), ms);
    };
}
```

Or use `useRef` for the timer to avoid re-creating:

```tsx
const saveTimer = useRef<ReturnType<typeof setTimeout>>();

const handleContentInput = (e: React.ChangeEvent<HTMLTextAreaElement>) => {
    const value = e.target.value;
    setContent(value);
    clearTimeout(saveTimer.current);
    saveTimer.current = setTimeout(() => {
        updateNote(note.id, { content: value });
    }, 300);
};
```

#### 5.4 — Sync position & size back to store

```tsx
import { getCurrentWindow } from '@tauri-apps/api/window';

useEffect(() => {
    const win = getCurrentWindow();

    const unlistenMove = win.onMoved(({ payload }) => {
        updateNote(note.id, { x: payload.x, y: payload.y });
    });

    const unlistenResize = win.onResized(({ payload }) => {
        updateNote(note.id, { width: payload.width, height: payload.height });
    });

    return () => {
        unlistenMove.then(f => f());
        unlistenResize.then(f => f());
    };
}, [note.id]);
```

#### 5.5 — Close and Delete handlers

```tsx
import { getCurrentWindow } from '@tauri-apps/api/window';
import { updateNote, deleteNote } from '../utils/store';

const handleClose = async () => {
    await getCurrentWindow().hide();
};

const handleDelete = async () => {
    await deleteNote(note.id);
    await getCurrentWindow().destroy();
};
```

### Verification Checklist

- [ ] Note window shows a styled header with "ToDo" title
- [ ] Title is editable and saves to store on blur
- [ ] Textarea fills the body, auto-saves with debounce
- [ ] Close button hides the window; Delete button removes from store + destroys window
- [ ] Action buttons appear only on header hover
- [ ] Moving and resizing the window persists position/size to store

---

## Phase 6: Add Note Button

**Goal:** Floating `+` button pinned to bottom-right of screen; clicking it creates a new note.

### Steps

#### 6.1 — Spawn the `+` button window

Add to Rust `.setup()` (after note restoration):

```rust
// Spawn the add-button window
let (sw, sh) = {
    // Get primary monitor size
    let monitor = app.primary_monitor()?.unwrap();
    (monitor.size().width, monitor.size().height)
};

tauri::WebviewWindowBuilder::new(
    app,
    "add-button",
    tauri::WebviewUrl::App("index.html#add-button".into()),
)
.title("Add Note")
.inner_size(60.0, 60.0)
.position(sw as f64 - 80.0, sh as f64 - 80.0)
.resizable(false)
.decorations(false)
.transparent(true)
.always_on_top(true)
.skip_taskbar(true)
.build()
.ok();
```

#### 6.2 — Add Button UI component

`src/components/AddButtonWindow.tsx`:

```tsx
import { invoke } from '@tauri-apps/api/core';
import { getCurrentWindow } from '@tauri-apps/api/window';

export default function AddButtonWindow() {
    const handleClick = async () => {
        // Call Rust command to create a new note
        await invoke('create_note');
    };

    // Also need click-through toggle (same as Phase 4)
    useEffect(() => {
        getCurrentWindow().setIgnoreCursorEvents(true);
        const el = document.getElementById('add-btn');
        el?.addEventListener('mouseenter', () => {
            getCurrentWindow().setIgnoreCursorEvents(false);
        });
        el?.addEventListener('mouseleave', () => {
            getCurrentWindow().setIgnoreCursorEvents(true);
        });
    }, []);

    return (
        <div
            id="add-btn"
            className="add-button"
            onClick={handleClick}
            title="Add Note"
        >
            +
        </div>
    );
}
```

`src/styles/global.css` (add):

```css
.add-button {
    width: 100%;
    height: 100%;
    display: flex;
    align-items: center;
    justify-content: center;
    font-size: 32px;
    font-weight: bold;
    color: #333;
    background: rgba(255, 255, 255, 0.9);
    border-radius: 50%;
    cursor: pointer;
    box-shadow: 0 2px 12px rgba(0, 0, 0, 0.2);
    transition: transform 0.1s ease, box-shadow 0.1s ease;
    user-select: none;
}

.add-button:hover {
    transform: scale(1.1);
    box-shadow: 0 4px 20px rgba(0, 0, 0, 0.3);
}
```

#### 6.3 — Offset new note position

In the Rust `create_note` command (Phase 3.5), calculate offset:

```rust
let offset = notes.len() as f64 * 30.0;
let x = 100.0 + offset;
let y = 100.0 + offset;
```

Or smarter: find the rightmost/bottom-most note and offset from there.

### Verification Checklist

- [ ] A floating `+` button appears at the bottom-right of the screen
- [ ] The `+` button is click-through when not hovered
- [ ] Hovering the `+` button makes it clickable (shows hover effect)
- [ ] Clicking `+` creates a new note window with "ToDo" title
- [ ] New notes are offset so they don't fully overlap existing notes
- [ ] The `+` button persists across app restarts

---

## Cross-Cutting Concerns

### Color Theming
- Each note has a `color` field (hex)
- Default colors: `#FFF9C4` (yellow), `#C8E6C9` (green), `#BBDEFB` (blue), `#F8BBD0` (pink), `#E1BEE7` (purple)
- Color picker can be added to the header or Preferences later

### Error Handling
- All Tauri `invoke()` calls should have `.catch()` fallbacks
- Store operations should handle missing/corrupted files gracefully
- Window creation failures should not crash the app

### App Icon
- Generate with: `npx tauri icon path/to/icon.png`
- Minimum 1024x1024 source PNG for all platforms

### Build Configuration
- `tauri.conf.json` → `bundle.identifier` should be unique
- Consider adding `bundle.windows.webviewInstallMode` for Windows deployment
- Set `productName` and `version` before first build

---

## Known Gotchas & Edge Cases

1. **WebviewWindow URL routing** — Since all windows load the same `index.html`, the frontend MUST check `window.location.hash` AND `window.label` to decide what to render. Hash-based routing is the most reliable.

2. **Click-through race conditions** — If the user moves the mouse very fast, `elementFromPoint` may lag. Solution: also use `mouseenter`/`mouseleave` on specific elements to force `setIgnoreCursorEvents(false)`.

3. **Window close vs destroy** — `hide()` keeps the window in memory (fast to restore); `destroy()` frees it entirely. Use `hide()` for Close, `destroy()` for Delete.

4. **Store file location** — `notes.json` is saved in the OS app data directory:
   - Windows: `%APPDATA%\com.sticky-notes.app\notes.json`
   - macOS: `~/Library/Application Support/com.sticky-notes.app/notes.json`
   - Linux: `~/.local/share/com.sticky-notes.app/notes.json`

5. **Multiple monitors** — Note positions may be off if saved on one monitor and restored on another with different DPI. Consider normalizing positions relative to the primary monitor.

6. **Transparent windows on Windows** — Windows may require `--disable-gpu-compositing` flag for proper transparency. If transparency doesn't work, test with Tauri's `additionalBrowserArgs`.

7. **Global shortcut on macOS** — macOS requires the app to be in the Accessibility permissions list for global shortcuts. This is not needed on Windows/Linux.

8. **`setIgnoreCursorEvents` persistence** — When a window is hidden and re-shown, the ignore state resets to `false`. Always re-set it after showing a window.

9. **Concurrent store writes** — Multiple note windows writing to the store simultaneously is safe with `tauri-plugin-store` — it serializes writes internally. But debounce the writes to avoid excessive I/O.

10. **`data-tauri-drag-region`** — This attribute must be on the draggable element AND the CSS must set `cursor: grab` for the drag to work properly on some platforms.

---

## Implementation Order Summary

| Phase | What to implement | Estimated complexity |
|-------|-------------------|---------------------|
| **1** | Scaffold, models, store, background-only lifecycle | Medium |
| **2** | Tray icon + menu, Preferences window, frontend router | Medium |
| **3** | Global shortcut, spawn windows, restore on launch | Medium-High |
| **4** | Click-through via mousemove + setIgnoreCursorEvents | Low-Medium |
| **5** | Note UI (header, title, body, actions, auto-save, sync) | Medium |
| **6** | Add button window, new note creation flow | Low |

---

> **How to use this plan in future sessions:**
> 1. Tell the implementing agent: *"Read IMPLEMENTATION_PLAN.md and implement Phase X"*
> 2. The agent should implement only that phase's steps
> 3. Verify using that phase's checklist
> 4. Confirm before moving to the next phase
