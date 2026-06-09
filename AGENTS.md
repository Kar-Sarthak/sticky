# Sticky Notes — Agent Guide

## What This Is

Tauri 2 desktop app (React + TypeScript frontend, Rust backend). Each sticky note is its own WebviewWindow — there is no main window. The app shows/hides notes via a global hotkey and system tray.

## Key Commands

```bash
# Frontend dev (Vite on port 1420, strict port)
npm run dev

# Full Tauri dev (frontend + Rust backend)
npm run tauri dev

# Build
npm run build          # tsc + vite build
npm run tauri build    # Full desktop app build

# No test suite, no linter, no formatter configured
# TypeScript strict mode is enabled (noUnusedLocals, noUnusedParameters)
npx tsc --noEmit       # Type check only
```

## Architecture

**Window routing** — `src/main.tsx` reads `getCurrentWindow().label` to pick the component:
- `note-*` → `NoteWindow`
- `preferences` → `PreferencesWindow`
- `todo-popup-*` → `TodoPopupWindow`

**Persistence** — `tauri-plugin-store` JSON files (managed by Rust):
- `notes.json` — notes array + hotkey string
- `todos.json` — all todo items globally
- `contexts.json` — todo-to-context mapping

**Data model** — `src-tauri/src/models.rs`:
- `Note` has `todo_ids: Vec<String>` (references into global todos store)
- `TodoItem` has `id`, `task`, `status` ("undone"|"done")

**Tauri commands** (registered in `src-tauri/src/lib.rs`):
`create_note`, `add_todo`, `toggle_todo`, `delete_todo`, `delete_note_todos`, `get_note_todos`, `spawn_preferences_window`, `re_register_shortcut`, `note_hidden`, `slide_left_and_destroy_popup`

## Python Services

Two optional Python scripts run alongside the app:

- `src-tauri/todo_context_server.py` — HTTP server on port 8765. Classifies todo text into app/website contexts using Groq AI API. Requires `GROQ_API_KEY` in `src-tauri/.env`.
- `todo_monitor.py` — Monitors active window/URL and shows todo popups when context matches. Uses win32gui, psutil, uiautomation (Windows-only).

Both are optional; the app works without them.

## Gotchas

- **No main window** — `tauri.conf.json` has `"windows": []`. The app only creates windows programmatically.
- **Window decorations disabled** — note windows have no title bar; they're draggable via custom CSS regions.
- **Global shortcut** defaults to `Ctrl+Shift+S`. Stored in `notes.json` as `hotkey`. Re-registerable from Preferences.
- **Popup windows** slide in from off-screen (x=-500) with bounce animation, managed by complex atomic state in Rust.
- **Context classification** fires asynchronously with 3 retries — don't block on it.
- **Single Rust binary** — all logic is in `src-tauri/src/lib.rs` (1458 lines). This is the core file to understand.
- **React 19** — not 18. `@tauri-apps/api` v2, not v1.
