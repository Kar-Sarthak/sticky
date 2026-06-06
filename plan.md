I'm building a Tauri 2 sticky note app. Here is the full architecture plan:

Here is the updated implementation plan, fully revised around the separate-`WebviewWindow`-per-note architecture:

## Phase 1: Project Foundation

1. **Initialize Tauri 2 project** — Scaffold with your chosen frontend framework (React/Svelte/Vue), configure `Cargo.toml` with feature flags (`tray-icon`, `image-png`, `global-shortcut`), and add `"core:webview:allow-create-webview-window"` to your capabilities file
2. **Define note data model** — Create a TypeScript interface and Rust struct for a note: `{ id, title, content, x, y, width, height, color }`; this schema drives both the store and the window spawn logic
3. **Persistence layer** — Wire up `tauri-plugin-store` to save and load the notes array; all window state (position, size) must be persisted here so notes survive app restarts
4. **App lifecycle: background-only** — Configure `tauri.conf.json` to start with no main window, and intercept the close event on every window so it hides rather than quits the process

***

## Phase 2: System Tray

5. **Tray icon & menu** — Register a `TrayIconBuilder` in the Rust `setup()` hook with a right-click menu containing "Show Notes", "Preferences", and "Quit"
6. **Preferences window** — Spawn a small dedicated `WebviewWindow` (label: `preferences`) from the tray menu for settings; this window has normal decorations and is not always-on-top
7. **Hotkey preference UI** — Add a hotkey capture input inside the Preferences window that reads and writes the chosen shortcut to the store

***

## Phase 3: Global Hotkey & Note Window Spawning

8. **Global shortcut registration** — Use `tauri-plugin-global-shortcut` to register `Ctrl+Shift+S` on startup; the handler should show all existing note windows or hide them if already visible (toggle behavior)
9. **Re-register shortcut on change** — When the user saves a new hotkey in Preferences, unregister the old shortcut and register the new one immediately without restarting the app
10. **Note window factory function** — Write a reusable frontend function `spawnNoteWindow(note)` that creates a `WebviewWindow` with label `note-${id}`, `decorations: false`, `transparent: true`, `alwaysOnTop: true`, `resizable: true`, and the note's saved `x/y/width/height`
11. **Restore notes on launch** — On app start, read all notes from the store and call `spawnNoteWindow()` for each one, rehydrating the full previous session

***

## Phase 4: Click-Through Behavior

12. **mousemove toggle pattern** — Inside each note's webview, add a `mousemove` listener that calls `getCurrentWindow().setIgnoreCursorEvents(true/false)` depending on whether `document.elementFromPoint()` returns a real UI element or the transparent background; this is the core click-through mechanism
13. **Disable click-through during interaction** — Ensure `setIgnoreCursorEvents(false)` is always set when the user is actively typing, dragging, or resizing so input is never accidentally swallowed

***

## Phase 5: Note UI

14. **Custom header bar** — Build an HTML header bar with `data-tauri-drag-region` for dragging; default title text is "ToDo"
15. **Hover-reveal action buttons** — Use CSS `:hover` on the header to show **Close** (hides the window) and **Delete** (removes from store + destroys the window via `window.destroy()`) buttons
16. **Editable title** — Make the header title a `contenteditable` span or `<input>` that auto-saves to the store on `blur`
17. **Note body textarea** — A `<textarea>` filling the note body with no border; auto-save content to the store on every `input` event with a short debounce (e.g. 300ms)
18. **Sync position & size back to store** — Listen to Tauri's `tauri://move` and `tauri://resize` window events inside each note and update the store so the layout persists

***

## Phase 6: Add Note Button

19. **Floating "+" overlay window** — Create one persistent, separate `WebviewWindow` (label: `add-button`) pinned to the bottom-right of the screen containing only the `+` button; this window is also `transparent`, `alwaysOnTop`, and `decorations: false`, and uses the same click-through toggle so it doesn't block the desktop
20. **New note creation flow** — Clicking `+` generates a new note ID, writes a default note object to the store, and calls `spawnNoteWindow()` with a slightly offset position from the last note to avoid full overlap

***



We will implement this ONE PHASE AT A TIME. Do not implement anything 
beyond the current phase I ask for. After each phase, wait for my confirmation 
before proceeding.

Tech stack: Tauri 2, [React/Svelte/Vue], TypeScript, Rust