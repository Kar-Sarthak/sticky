# Sticky Notes App — Complete Project Reference

> **How to use this file:** Hand it to any new session. It contains the complete current state of the project — every file, every feature, every architecture decision. No prior context needed.

---

## Table of Contents

1. [Overview](#1-overview)
2. [Tech Stack](#2-tech-stack)
3. [Project Structure](#3-project-structure)
4. [Data Model](#4-data-model)
5. [File-by-File Details](#5-file-by-file-details)
6. [Features](#6-features)
7. [UI/UX Design](#7-uiux-design)
8. [Key Architecture Decisions](#8-key-architecture-decisions)
9. [Important Patterns & Gotchas](#9-important-patterns--gotchas)
10. [Build & Run Commands](#10-build--run-commands)

---

## 1. Overview

A Tauri 2 desktop sticky notes app that spawns **one separate WebviewWindow per note**. The app runs background-only (no main window), with a system tray for control, global keyboard shortcut to toggle note visibility, and realistic sticky note styling (vibrant colors, handwritten font, ruled paper lines, layered shadows).

Each note is a **todo list** — pressing Enter creates a new todo item. Todos are stored globally in `todos.json` and linked to notes via ID references. Each note stores an ordered array of `todoIds` pointing to its todos.

Notes persist position, size, title, color, and todo references across restarts via `tauri-plugin-store` (JSON files).

A **context-aware reminder window** monitors the active window, matches todos by context (e.g., "github", "gmail"), and slides down from the top of the screen to show matching undone todos for the current window.

---

## 2. Tech Stack

| Layer | Technology | Version |
|-------|-----------|---------|
| Backend framework | Tauri 2 | ^2 |
| Backend language | Rust | Edition 2021 |
| Frontend framework | React 19 | ^19 |
| Frontend language | TypeScript | ~5.8 |
| Build tool | Vite | ^7 |
| Store persistence | tauri-plugin-store | 2.4.3 |
| Global shortcut | tauri-plugin-global-shortcut | 2.3.2 |
| UUID generation | uuid | ^1 (v4) |
| HTTP client | reqwest | ^0.12 |
| Async runtime | tokio | ^1 |
| HTTP server (Rust) | tiny_http | ^0.12 |
| Fonts | Permanent Marker (Google Fonts) | — |
| Platform | Windows 11 (primary target) | — |
| AI context classification | google-genai (Python) | gemini-3.1-flash-lite |

### Key Rust Dependencies (`src-tauri/Cargo.toml`)

```toml
tauri = { version = "2", features = ["tray-icon", "image-png"] }
tauri-plugin-store = "2.4.3"
tauri-plugin-global-shortcut = "2.3.2"
serde = { version = "1", features = ["derive"] }
serde_json = "1"
uuid = { version = "1", features = ["v4"] }
reqwest = { version = "0.12", features = ["json"] }
tokio = { version = "1", features = ["rt"] }
tiny_http = "0.12"
```

### Key npm Dependencies (`package.json`)

```json
"@tauri-apps/api": "^2",
"@tauri-apps/plugin-store": "^2",
"react": "^19", "react-dom": "^19"
```

---

## 3. Project Structure

```
e:\sticky\
├── package.json                          # npm deps, scripts
├── index.html                            # Vite entry, Google Fonts link (Permanent Marker)
├── vite.config.ts                        # Vite config (port 1420)
├── tsconfig.json                         # TypeScript config
├── src/
│   ├── main.tsx                          # Frontend router: detects window label → mounts correct component
│   ├── types.ts                          # TypeScript Note + TodoItem interfaces
│   ├── components/
│   │   ├── NoteWindow.tsx                # Note UI: todo list, checkboxes, grip, color picker
│   │   ├── PreferencesWindow.tsx         # Preferences UI (hotkey input + save)
│   │   ├── ReminderWindow.tsx            # Context-aware reminder: slides down with matching todos
│   │   └── AddButtonWindow.tsx           # Floating + button (unused — add button in note header)
│   ├── styles/
│   │   ├── global.css                    # Global resets, Preferences styles, AddButton styles
│   │   ├── note.css                      # Note styles: header, grip, color picker, todo list, checkboxes
│   │   └── reminder.css                  # Reminder window styles: fade animation, checkbox
│   └── utils/
│       ├── store.ts                      # Notes store helpers: getNotes, updateNote, deleteNote, etc.
│       └── spawnWindow.ts                # Frontend spawn helpers (unused — spawning done from Rust)
│
├── src-tauri/
│   ├── Cargo.toml                        # Rust dependencies
│   ├── tauri.conf.json                   # Tauri config (no main window, background-only)
│   ├── build.rs                          # Standard Tauri build script
│   ├── todo_context_server.py            # Python: AI classification + window context monitor
│   ├── .env                              # Gemini API key (gitignored)
│   ├── capabilities/
│   │   └── default.json                  # Permissions for all windows
│   ├── icons/                            # Generated app/tray icons
│   └── src/
│       ├── main.rs                       # Entry point → calls lib::run()
│       ├── lib.rs                        # ALL Rust logic: setup, tray, hotkey, todo/reminder commands
│       └── models.rs                     # Note + TodoItem structs (Rust side, shared via serde)
```

---

## 4. Data Model

### Rust (`src-tauri/src/models.rs`)

```rust
/// A single todo item stored globally in todos.json.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TodoItem {
    pub id: String,       // UUID v4
    pub task: String,     // Todo text
    pub status: String,   // "undone" | "done"
}

/// A sticky note. Stores an ordered list of todo IDs.
/// The actual todo data lives in todos.json.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Note {
    pub id: String,           // UUID v4
    pub title: String,        // Default "ToDo"
    pub x: f64,               // Window position X
    pub y: f64,               // Window position Y
    pub width: f64,           // Window width (default 300)
    pub height: f64,          // Window height (default 200)
    pub color: String,        // Hex color, e.g. "#FFE066"
    pub todo_ids: Vec<String>,// Ordered list of todo IDs belonging to this note
}
```

### TypeScript (`src/types.ts`)

```ts
export interface Note {
  id: string;
  title: string;
  x: number;
  y: number;
  width: number;
  height: number;
  color: string;
  todoIds: string[];
}

export interface TodoItem {
  id: string;
  task: string;
  status: "undone" | "done";
}
```

### Persistence — THREE Store Files

All data is stored in the Tauri app data directory:

**`notes.json`** — Note metadata + todo references
```json
{
  "notes": [
    {
      "id": "uuid-1",
      "title": "Shopping",
      "x": 100, "y": 100, "width": 300, "height": 200,
      "color": "#FFE066",
      "todo_ids": ["uuid-a", "uuid-b"]
    }
  ],
  "hotkey": "CommandOrControl+Shift+S"
}
```

**`todos.json`** — All todos from all notes
```json
{
  "todos": [
    { "id": "uuid-a", "task": "Buy milk", "status": "undone" },
    { "id": "uuid-b", "task": "Call dentist", "status": "done" }
  ]
}
```

**`contexts.json`** — Context classification: maps context labels → todo IDs
```json
{
  "contexts": {
    "gmail": ["uuid-b", "uuid-d"],
    "shopping": ["uuid-a"],
    "vscode": ["uuid-c"]
  }
}
```

Populated asynchronously by the Python context classifier server. When a new todo is created, its text is sent to the Gemini LLM which returns context labels (e.g., `["gmail", "calendar"]`). Those labels and the todo ID are saved to `contexts.json`.

**Store file location (Windows):** `%APPDATA%\com.sticky-notes.app\`

---

## 5. File-by-File Details

### `src-tauri/src/lib.rs` — Core Rust Logic

**State structs:**
- `GlobalShortcutState` — `Mutex<Option<Shortcut>>`, tracks the current registered shortcut so it can be unregistered when the user changes it in Preferences
- `NotesVisibility` — `Mutex<bool>`, tracks whether notes are supposed to be visible (true) or hidden (false). Close/delete on a single window doesn't change this — only the hotkey toggles it.
- **Reminder window state** — `Arc<AtomicBool>`: `false` = window is up/off-screen, `true` = window is down/visible. Prevents redundant slide-up animations.

**Functions:**
- `toggle_note_windows(app)` — Reads `NotesVisibility`. If true: hides all note windows and sets to false. If false: shows all and sets to true
- `parse_hotkey_string(s)` — Parses strings like "CommandOrControl+Shift+S" into a `Shortcut`. Supports A-Z, F1-F12, Space, Enter, Escape, Tab, Backspace, Delete, Arrow keys
- `get_all_todos(app)` → `Result<Vec<TodoItem>>` — reads all todos from `todos.json`
- `save_all_todos(app, todos)` → `Result<()>` — writes all todos to `todos.json`
- `get_contexts(app)` → `Result<HashMap<String, Vec<String>>>` — reads context mapping from `contexts.json`
- `save_contexts(app, ctx)` → `Result<()>` — writes context mapping to `contexts.json`
- `remove_todo_from_contexts(app, todo_id)` → `Result<()>` — removes a todo ID from all contexts, cleans up empty context keys
- `add_contexts(app, todo_id, contexts)` → `Result<()>` — adds todo IDs to context labels in `contexts.json`
- `spawn_context_server()` — Spawns `todo_context_server.py` as a detached background subprocess (stdio → null) at app startup. Checks if script exists and `google-genai` is installed first.
- `classify_todo_async(todo_id, task, app)` — Sends todo text to the Python server via HTTP POST, retries up to 3 times with 500ms delays, saves returned context labels to `contexts.json`
- `spawn_note_window(app, note)` — Creates a `WebviewWindow` with label `note-{id}`, loads `index.html#note-{id}`, sets `decorations: false`, `always_on_top: true`, `resizable: true`, with saved position/size
- `spawn_notes_on_launch(app)` — Reads all notes from store, calls `spawn_note_window` for each
- `spawn_reminder_window(app)` — Creates the reminder window at startup, positioned off-screen (y = -250). **After build, explicitly calls `set_position()` again to override any OS position clamping.**
- `animate_window_y(app, from_y, to_y)` — Animates reminder window Y position in 20 steps with ease-out cubic easing (~300ms total). Runs in a separate background thread.
- `show_reminder(app, todos, context)` — Sets reminder state flag to `true` (window is down), sends todo data, spawns animation thread to slide down
- `clear_reminder(app)` — Uses atomic `compare_exchange(true, false)` to check if window is currently down. If already up (false), returns immediately without animating. Otherwise slides up.
- `start_reminder_http_server(app)` — Starts `tiny_http` on port 8766 to receive reminder requests from Python monitor

**Tauri commands:**
- `spawn_preferences_window(app)` — Opens or focuses a 420x320 decorated window with hash `#preferences`
- `create_note(app)` — Generates a new Note with random position (20% safe zone margins), random color from 6-note palette, empty `todo_ids`, persists to store, spawns window, sets `NotesVisibility = true`
- `re_register_shortcut(app, new_hotkey)` — Unregisters old shortcut via `GlobalShortcutState`, parses new one, registers it, persists to store
- `note_hidden(app, is_destroying)` — Called when a note is closed (hide) or deleted (destroy). Checks visible note window count. If 0 remain visible (close) or only self was visible (destroy), sets `NotesVisibility = false`
- `add_todo(app, note_id, task)` — Creates a TodoItem in `todos.json`, adds its ID to the note's `todo_ids` in `notes.json`. **Also spawns `classify_todo_async` in background** for AI context classification
- `toggle_todo(app, todo_id)` — Flips status between "done" and "undone", emits `todo-updated` event to all windows
- `delete_todo(app, todo_id)` — Removes todo from `todos.json`, removes its ID from all notes' `todo_ids`, **and removes it from `contexts.json`**
- `delete_note_todos(app, note_id)` — Removes ALL todos belonging to a specific note from `todos.json` **and from `contexts.json`**
- `get_note_todos(app, note_id)` — Returns todos for a specific note, ordered by the note's `todo_ids` array
- `check_context_todos_and_slide(app, contexts)` — **New:** Reads `contexts.json` to find todo IDs matching the given contexts, then checks `todos.json` to count undone matching todos. If count is 0, calls `clear_reminder()` to slide up.

**Reminder HTTP endpoints (port 8766):**
- `POST /remind` — Receives todos from Python, shows reminder window (slide-down animation)
- `POST /slide-up` — Slides reminder window up off-screen

**Reminder HTTP endpoints (port 8766):**
- `POST /remind` — Receives todos from Python, shows reminder window (slide-down animation)
- `POST /slide-up` — Calls `clear_reminder()` to slide reminder up (only if window is currently down)

**`setup()` hook (app initialization):**
1. Initialize `notes.json` (create default yellow note if empty)
2. Initialize `todos.json` (create empty array if not exists)
3. Initialize `contexts.json` (create empty object if not exists)
4. **Spawn Python context classifier server** (`todo_context_server.py`)
5. **Spawn reminder window** (off-screen, with explicit position override)
6. **Start reminder HTTP server** (localhost:8766)
7. Restore all saved notes → spawn windows
8. Parse hotkey from store (default: Ctrl+Shift+S), register global shortcut
9. Create `GlobalShortcutState` and `NotesVisibility` managed state
10. **Initialize reminder state** (`Arc<AtomicBool::new(false)>` — starts as "up/off-screen")
11. Build system tray with menu: "Show Notes", "Preferences", separator, "Quit"
12. Left-click tray icon toggles note visibility
13. Tray menu handlers: "Show Notes" → toggle, "Preferences" → spawn window, "Quit" → exit
14. Register all 9 Tauri commands

**`on_window_event` — Global close interceptor:**
- On `CloseRequested`: calls `api.prevent_close()` then `_window.hide()` — notes hide instead of closing the app

### `src-tauri/src/main.rs` (6 lines)

```rust
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
fn main() {
    sticky_notes_lib::run()
}
```

### `src-tauri/src/models.rs`

Contains `Note` and `TodoItem` structs with `serde` derives. Shared between Rust and TypeScript.

### `src-tauri/todo_context_server.py`

Python server that does two things: (1) classifies todo text into app/website contexts via Gemini API, and (2) monitors the active window to show context-aware todo reminders.

**HTTP Server (localhost:8765):**
- `POST /classify` — Body: `{"text": "task description"}` → Response: `{"contexts": ["gmail", "calendar"]}`
- `GET /health` — Response: `{"status": "ok"}`

**Window Monitor (background thread):**
- Polls every 1 second to detect the active window (title + process name)
- For browsers (Firefox/Chrome/Edge), extracts the URL via `uiautomation`
- Maps process names → contexts (e.g., `code.exe` → `vscode`)
- Maps URL patterns → contexts (e.g., `github.com` → `github`)
- **Own app filtering:** Ignores windows from `sticky-notes.exe` so clicking on the reminder doesn't clear it
- **Context-set change detection:** Only triggers the reminder workflow when the **set of context labels** changes (e.g., `{"linkedin"}` → `{"github"}`). URL/title changes within the same context (e.g., LinkedIn jobs → LinkedIn notifications) do NOT trigger the workflow.
- **Reminder workflow on context change:**
  1. POST to `localhost:8766/slide-up` → reminder window slides up off-screen (only if currently visible)
  2. Check for matching undone todos in the new context
  3. If found: wait 350ms (for slide-up to complete), then POST to `localhost:8766/remind` with todos → reminder slides back down
  4. If all todos are done: don't send `/remind` (window stays off-screen)

**Dependencies:** `pip install google-genai pywin32 psutil uiautomation python-dotenv`
**API key:** Read from `src-tauri/.env` via `python-dotenv`.

---

### `src/components/ReminderWindow.tsx` — Context-Aware Reminder

A single persistent reminder window that shows matching todos for the current active window context.

**Behavior:**
- Spawns at startup, positioned off-screen (y = -250px)
- Slides down when matching todos are found, slides up when context changes
- Shows only **undone** todos — completed todos are filtered out
- Clicking a checkbox marks todo as done (calls `toggle_todo`), then fades out with strikethrough animation
- Styled identically to note todos (same checkbox, font, ruled lines)

**State:**
- `todos` — array of matching `TodoItem` from the current context
- `currentContext` — array of context labels for the current window (e.g., `["linkedin"]`)
- `fadingIds` — Set of todo IDs currently in the fade-out animation

**Auto slide-up on empty list:**
- After the 600ms fade-out animation removes a todo, checks if the list is now empty
- If empty, calls `invoke("check_context_todos_and_slide", { contexts: currentContext })`
- Rust reads `contexts.json` and `todos.json` to find matching todos for the current context
- If no undone todos remain for this context, triggers the slide-up animation

**CSS Animations:**
- `.fading` class: triggers strikethrough + color fade on text, opacity 1→0, slight upward slide
- After 600ms, the todo is removed from the list

---

### Reminder HTTP Server (Rust, localhost:8766)

A lightweight `tiny_http` server running inside the Tauri app that receives reminder requests from the Python monitor.

**Endpoints:**
- `POST /remind` — Body: `{"todos": [...], "context": "..."}` → Sends todo data to reminder window, then animates slide-down
- `POST /slide-up` — Calls `clear_reminder()` to animate the reminder window sliding up off-screen

**Animation system:**
- `animate_window_y(app, from_y, to_y)` — Moves window in 20 steps with ease-out cubic easing (15ms per step = ~300ms total)
- Runs in a **separate background thread** so the HTTP server stays responsive
- Constants: `REMINDER_OFF_SCREEN_Y = -250.0`, `REMINDER_ON_SCREEN_Y = 10.0`

**State tracking:**
- `Arc<AtomicBool>` flag tracks whether the reminder is currently visible (`true` = down/visible, `false` = up/off-screen)
- `show_reminder()` sets flag to `true`
- `clear_reminder()` uses `compare_exchange(true, false)` — if flag was already `false`, returns immediately without animating (prevents redundant slide-up animations)

---

### Context Matching Logic

**`CONTEXT_URL_MAP`** — Maps context names to URL keywords:
```python
"linkedin": ["linkedin.com"],
"github": ["github.com"],
"gmail": ["mail.google.com", "gmail.com"],
# ... 20+ contexts
```

**`CONTEXT_PROCESS_MAP`** — Maps process names to contexts:
```python
"code.exe": "vscode",
"slack.exe": "slack",
"discord.exe": "discord",
# ...
```

**`get_todos_for_contexts(active_contexts)`** — Loads `contexts.json` and `todos.json`, finds todos whose contexts overlap with the active window's contexts (case-insensitive fuzzy matching).

---

---

---

---

---

---

---

---

---

---

---

### `src-tauri/capabilities/default.json`

Permissions for all windows (`"*"`). Key permissions:
- `core:default`, `core:window:default`
- `core:window:allow-start-dragging`, `allow-hide`, `allow-show`, `allow-destroy`, `allow-set-focus`, `allow-is-visible`
- `core:webview:allow-create-webview-window`
- `core:tray:default`, `core:menu:default`
- `store:default`
- `global-shortcut:default`

### `src-tauri/tauri.conf.json`

- `"app": { "windows": [] }` — **No main window**, app starts in background
- `"app": { "withGlobalTauri": true }` — exposes `window.__TAURI__`
- `"identifier": "com.sticky-notes.app"`
- `"bundle": { "active": true, "targets": "all", "icon": [...] }`

### `src/main.tsx` — Frontend Router

Detects window label or URL hash and mounts the correct React component:
- `label.startsWith("note-")` or `hash.startsWith("#note-")` → `NoteWindow`
- `label === "preferences"` or `hash === "#preferences"` → `PreferencesWindow`
- `label === "reminder"` or `hash === "#reminder"` → `ReminderWindow`
- Fallback → "No UI loaded"

### `src/components/NoteWindow.tsx` — Main Note UI (Todo List)

**State:**
- `note` — current note data (id, title, color)
- `todos` — array of `TodoItem` for this note
- `noteCount` — total notes in store (polled every 500ms)
- `newTodoText` — text in the "add todo" input
- `showColorPicker` — whether color swatches are visible
- `editingTitle` — whether title is in edit mode
- `showTopFade` / `showBottomFade` — scroll indicator state

**Key behaviors:**
- **Title editing:** Click once → `onMouseDown` sets `contenteditable="true"` and selects all. Blur or Enter saves to store. Enter prevents newline
- **Color picker:** Click the color dot button → horizontal row of 6 color swatches appears to the right. Title is hidden while picker is open. Click a swatch → changes note color, saves to store, closes picker
- **Add todo:** Type in input at bottom, press Enter → calls `add_todo` (Rust), which creates todo in `todos.json` and adds ID to note's `todo_ids`. **Background:** todo text is sent to Python context server for AI classification; contexts are saved to `contexts.json` asynchronously (todo appears instantly)
- **Toggle todo:** Click checkbox → calls `toggle_todo` (Rust), flips status, re-renders with strikethrough for done items
- **Delete todo:** Hover a todo → ✕ button appears on right. Click → calls `delete_todo` (Rust), removes from `todos.json`, removes ID from note, **and removes from `contexts.json`**
- **Position/size sync:** Listens to `onMoved` and `onResized` Tauri events
- **Drag:** Grip dots on left call `getCurrentWindow().startDragging()` on mousedown
- **Close (✕):** Hides the window, calls `note_hidden` with `isDestroying: false`
- **Delete (🗑):** Calls `delete_note_todos` first (cleans up orphaned todos), then `note_hidden`, then `deleteNote`, then destroys window. Disabled when `noteCount <= 1`
- **New note (+):** Calls `create_note` Rust command
- **Real-time sync:** Listens for `todo-updated` events from Rust to refresh todos when toggled from another window (e.g., reminder)

### `src/components/PreferencesWindow.tsx`

Simple preferences UI:
- Text input for hotkey (default "CommandOrControl+Shift+S")
- Save button calls `invoke("re_register_shortcut", { newHotkey })`
- Shows "Saved!" or error feedback

### `src/components/AddButtonWindow.tsx`

Floating + button component. **Currently unused** — the add button was moved into each note's header. The window is no longer spawned from Rust.

### `src/utils/store.ts`

Notes store helpers using `@tauri-apps/plugin-store` for `notes.json`:
- `getStore()` — lazy-loaded singleton
- `getNotes()` — returns `Note[]`
- `saveNotes(notes)` — replaces entire array
- `addNote(note)` — appends
- `updateNote(id, updates)` — merges partial updates (used for title, color, todoIds, position)
- `deleteNote(id)` — removes by id

### `src/styles/note.css`

Complete note styling:
- `.note-container` / `.note-inner` — outer wrapper + inner content with border-radius, layered shadows
- `::before` on `.note-inner` — SVG turbulence noise overlay (paper texture at 4% opacity)
- `.note-header` — semi-transparent background, bottom border
- `.note-grip` — two columns of 3 dots each (6 total), cursor: grab
- `.note-title` — Permanent Marker font, left-aligned, editable on click
- `.color-picker-wrapper` / `.color-dot` / `.color-swatches` / `.color-swatch` — horizontal swatch row to the right
- `.btn-action` — transparent buttons with opacity transitions
- `.note-todo-list` — scrollable todo list container (scrollbar hidden)
- `.todo-item` — checkbox + text + hover-reveal delete, ruled line separator
- `.todo-item input[type="checkbox"]` — custom styled checkbox with checkmark
- `.todo-done` — strikethrough, faded text
- `.btn-todo-delete` — hidden by default, appears on hover at 50% opacity
- `.todo-new` / `.todo-new-input` — add todo input row
- `.note-fade-top` / `.note-fade-bottom` — scroll position indicators (6% alpha gradient)

### `src/styles/reminder.css`

Reminder window styling:
- `.reminder-window` — yellow background (#FFE066), rounded corners, shadow
- `.reminder-todo-item` — matches note todo styling (checkbox, font, ruled lines)
- `.reminder-todo-item.fading` — strikethrough + opacity fade animation
- `.reminder-checkbox` — native checkbox styled with border, checkmark, transitions

### `src/styles/global.css`

Global resets, Preferences window styles, AddButton styles.

### `index.html`

Links Google Fonts: `Permanent Marker` (handwritten font).

---

## 6. Features

### Implemented
- **Background-only lifecycle** — App starts with no window. System tray controls everything
- **Close → hide** — Clicking X on a window hides it, doesn't quit the app
- **System tray** — Icon with "Show Notes", "Preferences", "Quit" menu
- **Tray left-click** — Toggles all note windows show/hide
- **Global keyboard shortcut** — Default `Ctrl+Shift+S`, configurable in Preferences
- **Per-note WebviewWindow** — Each note is a separate window with its own URL hash
- **Restore on launch** — All saved notes reappear on app restart
- **Drag to move** — Via 6-dot grip handle on the left of the header
- **Resizable windows** — Users can resize note windows freely
- **Always on top** — Notes float above other windows
- **Title editing** — Click title to edit, blur/Enter saves
- **Position/size persistence** — Window position and size saved back to store
- **Color picker** — 6 vibrant sticky note colors, horizontal swatch row
- **Random colors** — New notes get a random color from the palette
- **Random positioning** — New notes appear in the center 60% of the screen (20% margins from all edges)
- **Todo list** — Each note is a todo list, not free text
- **Add todo** — Input at bottom, Enter creates new todo (saved globally + linked by ID)
- **Checkbox toggle** — Done/undone with strikethrough styling
- **Delete todo** — Hover-reveal ✕ button on each todo
- **Todo cleanup on note delete** — Deleting a note also deletes all its todos from `todos.json` **and removes their IDs from `contexts.json`**
- **AI context classification** — Every new todo is sent to Gemini AI (via Python server) which predicts the app/website context (e.g., "gmail", "linkedin", "vscode"). Contexts saved asynchronously to `contexts.json`. Todo appears instantly — classification happens in background
- **Delete protection** — Cannot delete the last note (button disabled)
- **Scroll indicators** — Subtle top/bottom fade gradients appear when todo list overflows
- **Ruled paper lines** — Horizontal rule line separators between todos
- **Handwritten font** — Permanent Marker from Google Fonts
- **Paper texture** — SVG turbulence noise overlay for realistic paper feel
- **Layered shadows** — Inset shadow (paper curl) + outer drop shadows, intensify on hover
- **Default note on first launch** — Creates a yellow "ToDo" note automatically
- **Toggle state sync** — `note_hidden` command ensures the hotkey toggle state stays correct when closing/deleting individual notes
- **Real-time todo sync** — `todo-updated` event emitted when any todo is toggled, all windows refresh their state
- **Context-aware reminder** — Single reminder window monitors active window context, slides down with matching undone todos, slides up on context change
- **Reminder animations** — Slide-down/slide-up with ease-out cubic easing (~300ms), fade-out with strikethrough when completing todos
- **Own app filtering** — Reminder ignores focus on our own app's windows (notes, reminder itself)
- **Auto slide-up on empty list** — When last todo in reminder is cleared, checks `contexts.json`/`todos.json` for remaining undone todos in current context; if none, slides up automatically
- **Redundant animation prevention** — Reminder state flag (`AtomicBool`) prevents slide-up animation if window is already off-screen
- **Context-set change detection** — Only triggers reminder workflow when context labels change, not on URL/title changes within the same context

### Not Implemented (removed during development)
- **Click-through behavior** — Was attempted (Phase 4) but caused drag conflicts, completely removed
- **Floating add button** — Center-screen + button when no notes exist, replaced by + button in note header
- **Add note window** — `AddButtonWindow.tsx` exists but is no longer spawned
- **Free-text textarea** — Replaced by todo list

---

## 7. UI/UX Design

### Note Appearance
- **Colors:** Yellow `#FFE066`, Green `#A8E6A1`, Blue `#87CEEB`, Pink `#FFB3C1`, Orange `#FFD4A1`, Purple `#D4B8E8`
- **Font:** Permanent Marker (cursive/handwritten)
- **Paper texture:** 4% opacity SVG noise filter overlay
- **Shadows:** Inset top shadow (paper curl feel) + 2-layer outer drop shadow
- **Hover:** Shadow intensifies slightly
- **Corner:** Simple 4px border-radius (no folded corner)

### Header Layout (left to right)
1. 6-dot drag grip (2 columns × 3 rows)
2. Color picker button (circular dot showing current color)
3. Title (editable, left-aligned)
4. Close (✕), Delete (🗑), New Note (+) — delete disabled when only 1 note

### Header Layout when color picker is open
1. Drag grip
2. Color picker button (open state)
3. **Title hidden** (makes room for horizontal swatch row)
4. Action buttons

### Todo List Layout
Each todo row: `[☐]` checkbox + todo text + [✕ delete on hover]
- Done items: `[✓]` checkbox + ~~strikethrough text~~ (faded)
- Bottom row: `[+]` "Add a todo..." input

### Context Classification Flow
1. User types todo text → presses Enter
2. Todo appears instantly in the list
3. Background task sends text to Python server (`localhost:8765/classify`)
4. Gemini returns context labels → saved to `contexts.json`

### Reminder Window Behavior
- **Off-screen by default** — Positioned at y = -250px (above the visible screen)
- **Slides down** — When matching undone todos are found for the current window context
- **Slides up** — When context changes, last todo is cleared and no undone todos remain for the current context, or user navigates away
- **Shows only undone** — Completed todos are filtered out
- **Checkbox action** — Marking done fades out with strikethrough, then triggers backend check for remaining context todos
- **No own-window clearing** — Clicking on the reminder or a note doesn't trigger context change
- **Startup position fix** — Explicit `set_position()` call after window build overrides any OS position clamping

---

## 8. Key Architecture Decisions

### One WebviewWindow per Note
Each note gets its own Tauri `WebviewWindow` with a unique label (`note-{uuid}`). The frontend router detects the window label and mounts `NoteWindow`. This enables:
- Independent positioning, sizing, and dragging
- Each note can be individually hidden/shown/destroyed
- Native window events (move, resize, close) per note

### Three-Store Architecture
Notes, todos, and contexts are stored in three separate files:
- **`notes.json`** — Note metadata + ordered `todo_ids` arrays. Light, fast to read.
- **`todos.json`** — All todo data globally (task text, status). Single source of truth.
- **`contexts.json`** — Context classification mapping: `{"gmail": ["todo-id-1", "todo-id-2"], "vscode": ["todo-id-3"]}`. Used for context-aware todo organization.

This design means:
- A todo's data (task text, status) lives in one place
- Notes reference todos by ID (no duplication)
- Context labels are stored separately from todo data, enabling efficient lookup by context
- Deleting a note cleans up both `todos.json` and `contexts.json`

### AI Context Classification
When a new todo is created:
1. Todo is immediately saved to `todos.json` and appears in the UI
2. Rust spawns `classify_todo_async` in the background (non-blocking)
3. The async task sends todo text to the Python HTTP server (`localhost:8765/classify`)
4. Python server calls Gemini API, returns context labels like `["gmail", "calendar"]`
5. Rust saves the contexts to `contexts.json`, mapping context → todo ID

The Python server is spawned at app startup as a detached background process. If it fails to start, context classification silently fails and the todo still works normally.

### Context Monitoring & Reminder Workflow
The Python `todo_context_server.py` runs a background monitor thread that:
1. **Polls** the active window every 1 second (title, process name, URL for browsers)
2. **Maps** process names and URLs to context labels (e.g., `github.com` → `github`)
3. **Detects context changes** by comparing the **set of context labels** — only triggers when the set changes (e.g., `{"linkedin"}` → `{"github"}`), not on URL changes within the same context
4. **On context change:** Sends `/slide-up` to Rust → reminder slides up off-screen (only if currently visible)
5. **Finds matching todos** in the new context via `get_todos_for_contexts()`
6. **If todos found:** Waits 350ms (for slide-up to complete), then sends `/remind` → reminder slides down
7. **If all todos done:** Doesn't send `/remind` (window stays off-screen)
8. **Own app filtering:** Ignores `sticky-notes.exe` so clicking on notes/reminder doesn't trigger context change

### Why No Main Window
`tauri.conf.json` has `"app": { "windows": [] }`. The app process starts in the background. The tray icon is the primary UI for showing/hiding notes and opening Preferences. This matches how sticky note apps work — they live in the background until summoned.

### Close → Hide, Not Quit
`on_window_event` intercepts `CloseRequested` and calls `api.prevent_close()` + `_window.hide()`. The Rust process stays running. "Quit" in the tray menu calls `app.exit(0)` for a clean exit.

### Toggle State Tracking
`NotesVisibility` (`Mutex<bool>`) tracks whether the hotkey should show or hide notes. Individual close/delete calls don't change this. The `note_hidden` command checks if the last visible note was just closed and updates the state so the next hotkey press correctly shows notes again. The `is_destroying` parameter handles the timing difference between `hide()` (already invisible) and `destroy()` (still visible when called).

### Color Consistency
The 6 color options are hardcoded in both Rust (`NOTE_COLORS` array in `lib.rs`) and TypeScript (`COLORS` array in `NoteWindow.tsx`). If you add/remove colors, update both.

---

## 9. Important Patterns & Gotchas

### 1. `note_hidden` Timing
- **Close (✕):** `hide()` runs first, then `note_hidden(is_destroying: false)` is called. `is_visible()` for this window returns false, so `visible_count == 0` means this was the last visible note
- **Delete (🗑):** `note_hidden(is_destroying: true)` runs BEFORE `destroy()`. The window is still visible, so `visible_count == 1` means only self was visible. After that, `destroy()` removes it

### 2. `noteCount` Polling
Each note window polls `getNotes()` every 500ms to keep `noteCount` in sync. This ensures the delete button correctly reflects the current state (enabled when 2+ notes, disabled when 1). Without polling, `noteCount` would be stale since it's only read once on mount.

### 3. Frontend Routing via Hash
Note windows load `index.html#note-{id}`. The router in `main.tsx` checks both `window.label` and `window.location.hash` to detect note windows. The hash approach ensures the correct note ID is available even before the store loads.

### 4. Todo Loading Order
On mount, `NoteWindow` calls `get_note_todos(noteId)` which returns todos in the order specified by the note's `todo_ids` array. This preserves the user's todo order across restarts.

### 5. Window Labels Are Unique
Each note window label is `note-{uuid}`. The spawn function checks `app.get_webview_window(&label).is_some()` to avoid duplicate windows.

### 6. No Click-Through
The click-through feature (Phase 4) was attempted but caused conflicts with the drag functionality and was completely removed. Notes now have normal hitboxes.

### 7. Hotkey Parsing
The `parse_hotkey_string` function in Rust supports specific modifiers and keys. If a user enters an invalid hotkey in Preferences, `re_register_shortcut` returns an error string. The frontend displays this error.

### 8. Build Cache Size
The `src-tauri/target/` directory grows to ~6GB. Run `cargo clean` in `src-tauri/` to reclaim space. The built app binary is only ~5-10MB.

### 9. Store Compatibility
When the `Note` struct changes (e.g., removing `content` field, adding `todo_ids`), old `notes.json` files become incompatible. Delete the store file to start fresh: `%APPDATA%\com.sticky-notes.app\notes.json`

### 10. Python Context Server
- Requires `pip install google-genai` and a valid Gemini API key in `src-tauri/.env`
- Server is spawned at app startup via `Stdio::inherit()` — output goes to the same terminal
- If the server isn't running, classify requests fail silently (todo still works, just no context labels)
- The server uses `gemini-3.1-flash-lite` model. Change `MODEL` in the Python file if needed

### 11. Async Classification
Todo appears instantly — context classification runs in the background. The `classify_todo_async` function retries up to 3 times with 500ms delays in case the Python server hasn't finished starting up yet.

### 12. Reminder Animation Threading
The `animate_window_y()` function runs in a **separate background thread** for each call. This prevents the HTTP server from blocking during the 300ms animation. The `time.sleep(0.35)` in Python ensures the slide-up completes before slide-down starts, preventing competing animations.

### 13. Real-Time Todo Sync
When `toggle_todo` is called, Rust emits a `todo-updated` event. All note windows and the reminder window listen for this event and refresh their todo lists. This ensures checkboxes stay in sync across all windows.

### 14. Reminder State Tracking
An `Arc<AtomicBool>` flag (`false` = up/off-screen, `true` = down/visible) prevents redundant slide-up animations. `clear_reminder()` uses `compare_exchange(true, false)` to check if the window is currently down; if already up, it returns immediately without animating.

### 15. Context-Set Change Detection
The monitor only triggers the reminder workflow when the **set of context labels** changes (e.g., `{"linkedin"}` → `{"github"}`). Navigating between different pages of the same website (e.g., LinkedIn jobs → LinkedIn notifications) does NOT trigger the workflow, preventing unnecessary slide-up/slide-down animations.

### 16. Auto Slide-Up on Empty List
When the last todo in the reminder fades out, the frontend calls `check_context_todos_and_slide(contexts)`. Rust reads `contexts.json` to find all todo IDs for the current context, then checks `todos.json` to count how many are undone. If count is 0, it triggers the slide-up animation automatically.

### 17. Startup Position Override
Tauri/Windows may clamp negative Y positions to `0` during window creation. After building the reminder window, an explicit `set_position()` call forces it to the off-screen coordinate (-250) to ensure it starts properly hidden.

---

## 10. Build & Run Commands

```bash
# Install deps (run from project root e:\sticky)
npm install

# Python dependencies (for AI context classification + window monitoring)
pip install google-genai pywin32 psutil uiautomation python-dotenv

# Development (hot-reload for frontend + Rust rebuild)
npm run tauri dev

# Build release binary
npm run tauri build

# Clean Rust build cache (frees ~6GB)
cd src-tauri && cargo clean

# Type-check frontend only
npx tsc --noEmit

# Check Rust compilation only
cd src-tauri && cargo check
```

### Store File Locations
```
# Windows
%APPDATA%\com.sticky-notes.app\notes.json     ← notes + todo IDs
%APPDATA%\com.sticky-notes.app\todos.json     ← all todo data
%APPDATA%\com.sticky-notes.app\contexts.json  ← context → todo ID mapping

# To reset everything (delete all stores):
Remove-Item "$env:APPDATA\com.sticky-notes.app\notes.json" -Force
Remove-Item "$env:APPDATA\com.sticky-notes.app\todos.json" -Force
Remove-Item "$env:APPDATA\com.sticky-notes.app\contexts.json" -Force
```

---

## Color Palette Reference

```
Yellow  #FFE066  (default)
Green   #A8E6A1
Blue    #87CEEB
Pink    #FFB3C1
Orange  #FFD4A1
Purple  #D4B8E8
```

Defined in TWO places:
1. **Rust:** `src-tauri/src/lib.rs` → `const NOTE_COLORS: [&str; 6]`
2. **TypeScript:** `src/components/NoteWindow.tsx` → `const COLORS` array
