mod models;

use models::{Note, TodoItem};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use tauri::Emitter;
use tauri::Manager;
use tauri::menu::{Menu, MenuItemBuilder, PredefinedMenuItem};
use tauri::tray::{MouseButton, TrayIconBuilder, TrayIconEvent};
use tauri_plugin_global_shortcut::{
    Code, GlobalShortcutExt, Modifiers, Shortcut, ShortcutState,
};
use tauri_plugin_store::StoreExt;
use uuid::Uuid;

/// State to track the currently registered global shortcut so we can
/// unregister it when the user changes the hotkey in Preferences.
struct GlobalShortcutState {
    current: Mutex<Option<Shortcut>>,
}

/// Tracks whether notes should be visible (true) or hidden (false).
/// Close/hide on a single window doesn't change this — only the hotkey toggles it.
struct NotesVisibility {
    notes_visible: Mutex<bool>,
}

/// Tracks whether popup bounce is active (for hover-to-reveal interruption).
struct PopupBounceActive {
    active: AtomicBool,
}

/// Generation counter for popup bounce. Incremented on /slide-left.
/// Stale 3-second delay threads check this before starting bounce —
/// if the generation changed, they cancel themselves.
struct PopupBounceGen {
    gen: AtomicU32,
}

/// Tracks whether popup windows are currently expanded (hovered).
/// Polling thread checks cursor position and slides back if mouse left.
struct PopupExpanded {
    expanded: AtomicBool,
    polling_active: AtomicBool,
    slide_in_progress: AtomicBool,
}

/// Generation counter for popup slide animations. Incremented on every
/// slide command (/slide-right, /slide-left, /slide-left-popup, checkbox destroy).
/// Old slide threads check this inside their loop and abort if stale —
/// prevents the "tug-of-war" when a new slide command arrives mid-animation.
struct PopupSlideGen {
    gen: AtomicU32,
}

/// Maps popup label → measured height. Used to compute dynamic Y positions
/// with correct gaps when each popup reports its size after font loading.
struct PopupHeights {
    map: Mutex<HashMap<String, f64>>,
}

/// Toggle visibility of all note windows.
/// If notes_visible == true → hide all notes, set notes_visible = false.
/// If notes_visible == false → show all notes, set notes_visible = true.
fn toggle_note_windows(app: &tauri::AppHandle) {
    let Some(state) = app.try_state::<NotesVisibility>() else {
        return;
    };
    let mut visible = state.notes_visible.lock().expect("poisoned lock");

    if *visible {
        for (label, _) in app.webview_windows().iter() {
            if label.starts_with("note-") {
                if let Some(win) = app.get_webview_window(label) {
                    win.hide().ok();
                }
            }
        }
        *visible = false;
    } else {
        for (label, _) in app.webview_windows().iter() {
            if label.starts_with("note-") {
                if let Some(win) = app.get_webview_window(label) {
                    win.show().ok();
                    win.set_focus().ok();
                }
            }
        }
        *visible = true;
    }
}

/// Parse a hotkey string like "CommandOrControl+Shift+S" into a Shortcut.
fn parse_hotkey_string(s: &str) -> Option<Shortcut> {
    let parts: Vec<&str> = s.split('+').collect();
    if parts.len() < 2 {
        return None;
    }

    let mut modifiers = Modifiers::empty();
    let mut code: Option<Code> = None;

    for part in &parts[..parts.len() - 1] {
        let m = match *part {
            "Control" => Modifiers::CONTROL,
            "CommandOrControl" => Modifiers::CONTROL,
            "Alt" => Modifiers::ALT,
            "Shift" => Modifiers::SHIFT,
            "Super" => Modifiers::SUPER,
            _ => Modifiers::empty(),
        };
        modifiers |= m;
    }

    let key = parts.last().unwrap();
    if key.len() == 1 {
        if let Some(c) = key.chars().next() {
            code = match c.to_ascii_uppercase() {
                'A' => Some(Code::KeyA),
                'B' => Some(Code::KeyB),
                'C' => Some(Code::KeyC),
                'D' => Some(Code::KeyD),
                'E' => Some(Code::KeyE),
                'F' => Some(Code::KeyF),
                'G' => Some(Code::KeyG),
                'H' => Some(Code::KeyH),
                'I' => Some(Code::KeyI),
                'J' => Some(Code::KeyJ),
                'K' => Some(Code::KeyK),
                'L' => Some(Code::KeyL),
                'M' => Some(Code::KeyM),
                'N' => Some(Code::KeyN),
                'O' => Some(Code::KeyO),
                'P' => Some(Code::KeyP),
                'Q' => Some(Code::KeyQ),
                'R' => Some(Code::KeyR),
                'S' => Some(Code::KeyS),
                'T' => Some(Code::KeyT),
                'U' => Some(Code::KeyU),
                'V' => Some(Code::KeyV),
                'W' => Some(Code::KeyW),
                'X' => Some(Code::KeyX),
                'Y' => Some(Code::KeyY),
                'Z' => Some(Code::KeyZ),
                _ => None,
            };
        }
    } else {
        code = match *key {
            "F1" => Some(Code::F1),
            "F2" => Some(Code::F2),
            "F3" => Some(Code::F3),
            "F4" => Some(Code::F4),
            "F5" => Some(Code::F5),
            "F6" => Some(Code::F6),
            "F7" => Some(Code::F7),
            "F8" => Some(Code::F8),
            "F9" => Some(Code::F9),
            "F10" => Some(Code::F10),
            "F11" => Some(Code::F11),
            "F12" => Some(Code::F12),
            "Space" => Some(Code::Space),
            "Enter" => Some(Code::Enter),
            "Escape" => Some(Code::Escape),
            "Tab" => Some(Code::Tab),
            "Backspace" => Some(Code::Backspace),
            "Delete" => Some(Code::Delete),
            "ArrowUp" => Some(Code::ArrowUp),
            "ArrowDown" => Some(Code::ArrowDown),
            "ArrowLeft" => Some(Code::ArrowLeft),
            "ArrowRight" => Some(Code::ArrowRight),
            _ => None,
        };
    }

    match (code, modifiers) {
        (Some(c), m) => Some(Shortcut::new(Some(m), c)),
        _ => None,
    }
}

// ─── Todos store helpers ───

fn get_all_todos(app: &tauri::AppHandle) -> Result<Vec<TodoItem>, String> {
    let store = app.store("todos.json").map_err(|e| e.to_string())?;
    Ok(store
        .get("todos")
        .and_then(|v| serde_json::from_value::<Vec<TodoItem>>(v.clone()).ok())
        .unwrap_or_default())
}

fn save_all_todos(app: &tauri::AppHandle, todos: &[TodoItem]) -> Result<(), String> {
    let store = app.store("todos.json").map_err(|e| e.to_string())?;
    store.set("todos", serde_json::to_value(todos).map_err(|e| e.to_string())?);
    store.save().map_err(|e| e.to_string())
}

// ─── Contexts store helpers ───

fn get_contexts(app: &tauri::AppHandle) -> Result<HashMap<String, Vec<String>>, String> {
    let store = app.store("contexts.json").map_err(|e| e.to_string())?;
    Ok(store
        .get("contexts")
        .and_then(|v| serde_json::from_value::<HashMap<String, Vec<String>>>(v.clone()).ok())
        .unwrap_or_default())
}

fn save_contexts(app: &tauri::AppHandle, ctx: &HashMap<String, Vec<String>>) -> Result<(), String> {
    let store = app.store("contexts.json").map_err(|e| e.to_string())?;
    store.set("contexts", serde_json::to_value(ctx).map_err(|e| e.to_string())?);
    store.save().map_err(|e| e.to_string())
}

/// Remove a todo ID from all contexts. Cleans up empty context keys.
fn remove_todo_from_contexts(app: &tauri::AppHandle, todo_id: &str) -> Result<(), String> {
    let mut ctx = get_contexts(app)?;
    for ids in ctx.values_mut() {
        ids.retain(|id| id != todo_id);
    }
    // Remove empty context keys
    ctx.retain(|_, v| !v.is_empty());
    save_contexts(app, &ctx)
}

/// Add todo IDs to contexts.
fn add_contexts(app: &tauri::AppHandle, todo_id: &str, contexts: &[String]) -> Result<(), String> {
    let mut ctx = get_contexts(app)?;
    for c in contexts {
        ctx.entry(c.clone())
            .or_insert_with(Vec::new)
            .push(todo_id.to_string());
    }
    save_contexts(app, &ctx)
}

// ─── Python context server ───

const CONTEXT_SERVER_URL: &str = "http://127.0.0.1:8765/classify";

/// Spawn the Python context classifier server as a background process.
fn spawn_context_server() {
    use std::process::{Command, Stdio};

    let script = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("todo_context_server.py");

    // Check if the script exists
    if !script.exists() {
        eprintln!("[context] todo_context_server.py not found, skipping context classification");
        return;
    }

    // Start the server in the background
    let server_proc = Command::new("python")
        .arg(&script)
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .spawn();

    match server_proc {
        Ok(mut proc) => {
            // Give the server a moment to start up
            std::thread::sleep(std::time::Duration::from_millis(500));

            // Check if it's still alive
            match proc.try_wait() {
                Ok(Some(status)) => {
                    eprintln!("[context] server exited with status: {}", status);
                }
                _ => {
                    println!("[context] context classifier server started (pid: {})", proc.id());
                }
            }
        }
        Err(e) => {
            eprintln!("[context] failed to start context server: {}", e);
        }
    }
}

/// Send todo text to the context server for async classification.
async fn classify_todo_async(todo_id: String, task: String, app: tauri::AppHandle) {
    let client = reqwest::Client::new();

    let body = serde_json::json!({ "text": task });

    // Retry up to 3 times with delays (server might not be ready yet)
    for attempt in 0..3 {
        match client.post(CONTEXT_SERVER_URL).json(&body).send().await {
            Ok(resp) => {
                if resp.status().is_success() {
                    match resp.json::<serde_json::Value>().await {
                        Ok(data) => {
                            if let Some(contexts) = data["contexts"].as_array() {
                                let labels: Vec<String> = contexts
                                    .iter()
                                    .filter_map(|v| v.as_str().map(String::from))
                                    .collect();

                                if let Err(e) = add_contexts(&app, &todo_id, &labels) {
                                    eprintln!("[context] failed to save contexts: {}", e);
                                } else {
                                    println!("[context] classified todo {}: {:?}", todo_id, labels);
                                }
                            }
                        }
                        Err(e) => eprintln!("[context] failed to parse response: {}", e),
                    }
                    return; // Success, don't retry
                }
            }
            Err(e) => {
                eprintln!("[context] classify request failed (attempt {}): {}", attempt + 1, e);
            }
        }

        // Wait before retrying
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
    }
}

// ─── Tauri commands for todos ───

/// Add a new todo item to the global todos store and return it.
/// Spawns an async background task to classify the todo context.
#[tauri::command]
async fn add_todo(app: tauri::AppHandle, note_id: String, task: String) -> Result<TodoItem, String> {
    let mut todos = get_all_todos(&app)?;

    let id = Uuid::new_v4().to_string();
    let todo = TodoItem {
        id: id.clone(),
        task,
        status: "undone".to_string(),
        seen: false,
    };
    todos.push(todo.clone());
    save_all_todos(&app, &todos)?;

    // Add this todo's ID to the note's todo_ids
    let notes_store = app.store("notes.json").map_err(|e| e.to_string())?;
    let mut notes: Vec<Note> = notes_store
        .get("notes")
        .and_then(|v| serde_json::from_value::<Vec<Note>>(v.clone()).ok())
        .unwrap_or_default();

    if let Some(note) = notes.iter_mut().find(|n| n.id == note_id) {
        note.todo_ids.push(todo.id.clone());
        notes_store
            .set("notes", serde_json::to_value(&notes).map_err(|e| e.to_string())?);
        notes_store.save().map_err(|e| e.to_string())?;
    }

    // Spawn async context classification in the background
    let app_clone = app.clone();
    let task_text = todo.task.clone();
    tauri::async_runtime::spawn(async move {
        classify_todo_async(id, task_text, app_clone).await;
    });

    Ok(todo)
}

/// Toggle a todo's status between "done" and "undone".
#[tauri::command]
async fn toggle_todo(app: tauri::AppHandle, todo_id: String) -> Result<(), String> {
    let mut todos = get_all_todos(&app)?;

    let mut new_status = None;
    if let Some(todo) = todos.iter_mut().find(|t| t.id == todo_id) {
        if todo.status == "done" {
            todo.status = "undone".to_string();
            todo.seen = false;  // Reset seen when a todo is un-done
            new_status = Some(todo.status.clone());
        } else {
            todo.status = "done".to_string();
            new_status = Some(todo.status.clone());
        }
    }
    save_all_todos(&app, &todos)?;

    // Notify all windows to refresh their todo lists
    if let Some(status) = new_status {
        app.emit("todo-updated", serde_json::json!({ "todoId": todo_id, "status": status })).ok();
    }

    Ok(())
}

/// Delete a todo from the global todos store and remove its ID from all notes.
/// Also cleans up the todo from contexts.json.
#[tauri::command]
async fn delete_todo(app: tauri::AppHandle, todo_id: String) -> Result<(), String> {
    let mut todos = get_all_todos(&app)?;
    todos.retain(|t| t.id != todo_id);
    save_all_todos(&app, &todos)?;

    // Remove this todo's ID from every note that references it
    let notes_store = app.store("notes.json").map_err(|e| e.to_string())?;
    let mut notes: Vec<Note> = notes_store
        .get("notes")
        .and_then(|v| serde_json::from_value::<Vec<Note>>(v.clone()).ok())
        .unwrap_or_default();

    for note in &mut notes {
        note.todo_ids.retain(|id| *id != todo_id);
    }
    notes_store
        .set("notes", serde_json::to_value(&notes).map_err(|e| e.to_string())?);
    notes_store.save().map_err(|e| e.to_string())?;

    // Remove from contexts.json
    remove_todo_from_contexts(&app, &todo_id)
}

/// Delete all todos belonging to a specific note.
/// Also cleans up all those todos from contexts.json.
#[tauri::command]
async fn delete_note_todos(app: tauri::AppHandle, note_id: String) -> Result<(), String> {
    // Get the note's todo IDs
    let notes_store = app.store("notes.json").map_err(|e| e.to_string())?;
    let notes: Vec<Note> = notes_store
        .get("notes")
        .and_then(|v| serde_json::from_value::<Vec<Note>>(v.clone()).ok())
        .unwrap_or_default();

    let todo_ids = notes.iter()
        .find(|n| n.id == note_id)
        .map(|n| n.todo_ids.clone())
        .unwrap_or_default();

    if todo_ids.is_empty() {
        return Ok(());
    }

    // Remove those todos from todos.json
    let mut all_todos = get_all_todos(&app)?;
    all_todos.retain(|t| !todo_ids.contains(&t.id));
    save_all_todos(&app, &all_todos)?;

    // Remove each todo from contexts.json
    for id in &todo_ids {
        remove_todo_from_contexts(&app, id)?;
    }

    Ok(())
}

/// Get all todos for a specific note.
#[tauri::command]
async fn get_note_todos(app: tauri::AppHandle, note_id: String) -> Result<Vec<TodoItem>, String> {
    // Get the note to find its todo IDs
    let notes_store = app.store("notes.json").map_err(|e| e.to_string())?;
    let notes: Vec<Note> = notes_store
        .get("notes")
        .and_then(|v| serde_json::from_value::<Vec<Note>>(v.clone()).ok())
        .unwrap_or_default();

    let note = notes.iter().find(|n| n.id == note_id);
    let todo_ids = note.map(|n| n.todo_ids.clone()).unwrap_or_default();

    // Fetch all todos and filter to just the ones for this note
    let all_todos = get_all_todos(&app)?;
    let mut note_todos: Vec<TodoItem> = all_todos
        .into_iter()
        .filter(|t| todo_ids.contains(&t.id))
        .collect();

    // Sort todos in the same order as the note's todo_ids
    note_todos.sort_by_key(|t| todo_ids.iter().position(|id| id == &t.id).unwrap_or(usize::MAX));

    Ok(note_todos)
}

// ─── Window spawning ───

/// Spawn a single note WebviewWindow from a Note struct.
fn spawn_note_window(app: &tauri::AppHandle, note: &Note) {
    let label = format!("note-{}", note.id);

    // Skip if already exists
    if app.get_webview_window(&label).is_some() {
        return;
    }

    let url = format!("index.html#note-{}", note.id);

    tauri::WebviewWindowBuilder::new(
        app,
        &label,
        tauri::WebviewUrl::App(url.into()),
    )
    .title(&note.title)
    .inner_size(note.width, note.height)
    .position(note.x, note.y)
    .resizable(true)
    .decorations(false)
    .always_on_top(true)
    .build()
    .ok();
}

/// Spawn WebviewWindows for all saved notes on app launch.
fn spawn_notes_on_launch(app: &tauri::AppHandle) -> Result<(), Box<dyn std::error::Error>> {
    let store = app.store("notes.json")?;
    let notes: Vec<Note> = store
        .get("notes")
        .and_then(|v| serde_json::from_value::<Vec<Note>>(v.clone()).ok())
        .unwrap_or_default();

    for note in &notes {
        spawn_note_window(app, note);
    }

    Ok(())
}

/// Tauri command: spawn (or focus) the Preferences window.
#[tauri::command]
async fn spawn_preferences_window(app: tauri::AppHandle) -> Result<(), String> {
    if let Some(win) = app.get_webview_window("preferences") {
        win.show().map_err(|e| e.to_string())?;
        win.set_focus().map_err(|e| e.to_string())?;
        return Ok(());
    }

    tauri::WebviewWindowBuilder::new(
        &app,
        "preferences",
        tauri::WebviewUrl::App("index.html#preferences".into()),
    )
    .title("Preferences")
    .inner_size(420.0, 320.0)
    .resizable(false)
    .decorations(true)
    .build()
    .map_err(|e| e.to_string())?;

    Ok(())
}

/// Tauri command: create a new note, persist it, and spawn its window.
#[tauri::command]
async fn create_note(app: tauri::AppHandle) -> Result<Note, String> {
    let store = app.store("notes.json").map_err(|e| e.to_string())?;

    let mut notes: Vec<Note> = store
        .get("notes")
        .and_then(|v| serde_json::from_value::<Vec<Note>>(v.clone()).ok())
        .unwrap_or_default();

    // Position the note randomly within the safe center region
    // (20% margins from all four edges)
    let (screen_w, screen_h) = match app.primary_monitor() {
        Ok(Some(monitor)) => {
            let size = monitor.size();
            let scale = monitor.scale_factor();
            (size.width as f64 / scale, size.height as f64 / scale)
        }
        _ => (1920.0, 1080.0), // fallback
    };

    // Safe zone: 20% margin on all sides
    let margin_w = screen_w * 0.2;
    let margin_h = screen_h * 0.2;
    let safe_w = (screen_w * 0.6 - 300.0).max(0.0);
    let safe_h = (screen_h * 0.6 - 200.0).max(0.0);

    const NOTE_COLORS: [&str; 6] = [
        "#FFE066", // Yellow
        "#A8E6A1", // Green
        "#87CEEB", // Blue
        "#FFB3C1", // Pink
        "#FFD4A1", // Orange
        "#D4B8E8", // Purple
    ];

    // Use UUID bytes as random seed for positioning and color
    let uuid = Uuid::new_v4();
    let id = uuid.to_string();
    let uuid_val = uuid.as_u128();
    let x = margin_w + (uuid_val as f64 % 1000.0) / 1000.0 * safe_w;
    let y = margin_h + (((uuid_val >> 32) as f64 % 1000.0) / 1000.0) * safe_h;
    // Pick random color from the palette
    let color_idx = (uuid_val >> 48) as usize % NOTE_COLORS.len();
    let color = NOTE_COLORS[color_idx].to_string();

    let note = Note {
        id,
        title: "ToDo".to_string(),
        x: x.max(0.0),
        y: y.max(0.0),
        width: 400.0,
        height: 450.0,
        color,
        todo_ids: Vec::new(),
    };

    notes.push(note.clone());
    let notes_value = serde_json::to_value(&notes).unwrap();
    store.set("notes", notes_value);
    store.save().map_err(|e| e.to_string())?;

    spawn_note_window(&app, &note);

    // New notes are visible, so set the visibility state to true
    if let Some(state) = app.try_state::<NotesVisibility>() {
        *state.notes_visible.lock().expect("poisoned lock") = true;
    }

    Ok(note)
}

/// Tauri command: re-register the global shortcut with a new hotkey string.
#[tauri::command]
async fn re_register_shortcut(
    app: tauri::AppHandle,
    new_hotkey: String,
) -> Result<(), String> {
    let shortcut = parse_hotkey_string(&new_hotkey)
        .ok_or_else(|| format!("Invalid hotkey: {}", new_hotkey))?;

    // Unregister the previously registered shortcut (if any)
    if let Some(state) = app.try_state::<GlobalShortcutState>() {
        let guard = state.current.lock().map_err(|e| e.to_string())?;
        if let Some(old) = guard.as_ref() {
            app.global_shortcut()
                .unregister(old.clone())
                .map_err(|e| e.to_string())?;
        }
    }

    // Register the new shortcut
    let toggle_closure = shortcut.clone();
    app.global_shortcut()
        .on_shortcut(shortcut.clone(), move |app_handle, _shortcut, event| {
            if event.state == ShortcutState::Pressed {
                toggle_note_windows(app_handle);
            }
        })
        .map_err(|e| e.to_string())?;

    // Update tracked state
    if let Some(state) = app.try_state::<GlobalShortcutState>() {
        let mut guard = state.current.lock().map_err(|e| e.to_string())?;
        *guard = Some(toggle_closure);
    }

    // Persist to store
    let store = app.store("notes.json").map_err(|e| e.to_string())?;
    store.set("hotkey", new_hotkey);
    store.save().map_err(|e| e.to_string())?;

    Ok(())
}

/// Called when a note window is being hidden (✕) or destroyed (🗑).
/// For close: `hide()` already ran, so `is_visible()` returns false for this window.
/// For delete: `destroy()` hasn't run yet, so this window is still counted as visible.
///
/// visible_count == 0 → last note was closed, no notes remain visible → flip toggle
/// visible_count == 1 → either deleting the last note (self still visible) or closing
///   the only visible note → flip toggle in both cases
#[tauri::command]
async fn note_hidden(app: tauri::AppHandle, is_destroying: bool) {
    let visible_count = app.webview_windows().iter().filter(|(label, _)| {
        label.starts_with("note-")
            && app
                .get_webview_window(label)
                .is_some_and(|w| w.is_visible().unwrap_or(false))
    }).count();

    let should_flip = if is_destroying {
        visible_count <= 1  // only self visible, no others
    } else {
        visible_count == 0  // no notes visible at all
    };

    if should_flip {
        if let Some(state) = app.try_state::<NotesVisibility>() {
            *state.notes_visible.lock().expect("poisoned lock") = false;
        }
    }
}

// ─── Todo Popup Windows ───

const TODO_POPUP_WIDTH: f64 = 300.0;
const TODO_POPUP_HEIGHT: f64 = 30.0;
const TODO_POPUP_START_Y: f64 = 50.0;
const TODO_POPUP_SPACING: f64 = 50.0;

/// Simple percent-encoding for URL query parameters.
fn encode_uri_component(s: &str) -> String {
    let mut encoded = String::with_capacity(s.len() * 3);
    for c in s.chars() {
        match c {
            'A'..='Z' | 'a'..='z' | '0'..='9' | '-' | '_' | '.' | '~' | '!' | '$' | '\'' | '(' | ')' | '*' | ',' | ':' | ';' | '@' => {
                encoded.push(c);
            }
            ' ' => encoded.push_str("+"),
            c => {
                for byte in c.to_string().as_bytes() {
                    encoded.push_str(&format!("%{:02X}", byte));
                }
            }
        }
    }
    encoded
}

/// Destroy all existing todo-popup windows.
fn destroy_all_todo_popups(app: &tauri::AppHandle) {
    let popup_labels: Vec<String> = app
        .webview_windows()
        .iter()
        .filter(|(label, _)| label.starts_with("todo-popup-"))
        .map(|(label, _)| label.to_string())
        .collect();

    for label in popup_labels {
        if let Some(win) = app.get_webview_window(&label) {
            win.destroy().ok();
        }
    }
}

/// Animate a todo popup window's X position off-screen, then destroy it.
const TODO_POPUP_SLIDE_LEFT_X: f64 = -400.0;

fn slide_left_todo_popup(app: &tauri::AppHandle, label: &str) {
    let win = match app.get_webview_window(label) {
        Some(w) => w,
        None => return,
    };

    let from_x = match win.outer_position() {
        Ok(pos) => pos.x as f64,
        Err(_) => 0.0,
    };

    // Capture current generation — abort if a newer slide command arrives
    let captured_gen = app.try_state::<Arc<PopupSlideGen>>()
        .map(|g| g.gen.load(Ordering::SeqCst));

    let label = label.to_string();
    let app = app.clone();
    std::thread::spawn(move || {
        let steps = 20;
        let total_distance = TODO_POPUP_SLIDE_LEFT_X - from_x;
        let step_delay = std::time::Duration::from_millis(15);

        for i in 1..=steps {
            // Abort if a newer slide command invalidated us
            if let Some(gen) = captured_gen {
                if let Some(state) = app.try_state::<Arc<PopupSlideGen>>() {
                    if state.gen.load(Ordering::SeqCst) != gen { return; }
                }
            }
            let progress = i as f64 / steps as f64;
            let eased = 1.0 - (1.0 - progress).powi(3);
            let current_x = from_x + total_distance * eased;

            if let Some(win) = app.get_webview_window(&label) {
                if let Ok(pos) = win.outer_position() {
                    win.set_position(tauri::PhysicalPosition::new(current_x as i32, pos.y as i32)).ok();
                }
            }
            std::thread::sleep(step_delay);
        }

        // Destroy the window after animation completes
        if let Some(win) = app.get_webview_window(&label) {
            win.destroy().ok();
        }
    });
}

/// Spawn a todo popup window for each matching undone todo.
/// First destroys all existing popup windows, then spawns new ones stacked vertically.
fn spawn_todo_popup_windows(app: &tauri::AppHandle, todos: &[TodoItem]) {
    // Destroy existing popup windows first
    destroy_all_todo_popups(app);

    let x = -500i32;

    for (i, todo) in todos.iter().enumerate() {
        // Label encodes the todo ID so Rust can extract it without reading the URL
        let label = format!("todo-popup-{}", todo.id);
        let y = (TODO_POPUP_START_Y + (i as f64 * TODO_POPUP_SPACING)) as i32;

        // Encode task text for URL (percent-encoded)
        let encoded_task = encode_uri_component(&todo.task);
        // Pass seen status so frontend can apply pink background for unseen todos
        let seen_param = if todo.seen { "1" } else { "0" };
        let url = format!("index.html#todo-popup?seen={}&task={}", seen_param, encoded_task);

        tauri::WebviewWindowBuilder::new(
            app,
            &label,
            tauri::WebviewUrl::App(url.into()),
        )
        .title(&todo.task)
        .inner_size(TODO_POPUP_WIDTH, TODO_POPUP_HEIGHT)
        .position(x as f64, y as f64)
        .resizable(false)
        .decorations(false)
        .transparent(true)
        .always_on_top(true)
        .skip_taskbar(true)
        .visible(false)
        .build()
        .ok();

        // Force off-screen position to override any OS position clamping
        if let Some(win) = app.get_webview_window(&label) {
            win.set_position(tauri::PhysicalPosition::new(x, y)).ok();
            // Show the window now that it's at the correct off-screen position
            win.show().ok();
        }
    }

    // Start bounce animation for all popup windows (after 3s delay, matching reminder).
    // Capture current generation so a stale delay thread from a previous context
    // doesn't bounce windows for the wrong context.
    if let Some(gen) = app.try_state::<Arc<PopupBounceGen>>() {
        gen.gen.fetch_add(1, Ordering::SeqCst);
    }
    let captured_gen = app.try_state::<Arc<PopupBounceGen>>()
        .map(|g| g.gen.load(Ordering::SeqCst));
    let app = app.clone();
    std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_secs(3));
        // Check if a context change happened during the 3s delay
        if let Some(current_gen) = captured_gen {
            if let Some(gen_state) = app.try_state::<Arc<PopupBounceGen>>() {
                if gen_state.gen.load(Ordering::SeqCst) == current_gen {
                    start_popup_bounce(&app);
                }
                // Generation mismatch → stale thread, cancel silently
            }
        }
    });
}

/// Stop the popup bounce animation.
fn stop_popup_bounce(app: &tauri::AppHandle) {
    if let Some(state) = app.try_state::<Arc<PopupBounceActive>>() {
        state.active.store(false, Ordering::SeqCst);
    }
}

/// Start a bounce animation for all todo popup windows.
/// Bounces them in X: -380 ↔ -330 for 5 cycles, then stops (windows stay on screen).
fn start_popup_bounce(app: &tauri::AppHandle) {
    // Collect popup labels now so we can iterate in the bounce thread
    let popup_labels: Vec<String> = app
        .webview_windows()
        .iter()
        .filter(|(label, _)| label.starts_with("todo-popup-"))
        .map(|(label, _)| label.to_string())
        .collect();

    if popup_labels.is_empty() {
        return;
    }

    // Reset bounce flag
    if let Some(state) = app.try_state::<Arc<PopupBounceActive>>() {
        state.active.store(true, Ordering::SeqCst);
    }

    let app = app.clone();
    std::thread::spawn(move || {
        let base_x = -380.0;
        let amplitude = 50.0;

        // Bounce for exactly 5 cycles
        for _cycle in 0..5 {
            for step in 0..60 {
                if let Some(state) = app.try_state::<Arc<PopupBounceActive>>() {
                    if !state.active.load(Ordering::SeqCst) {
                        return;
                    }
                }
                let t = step as f64 / 60.0;
                let offset = amplitude * (1.0 - (2.0 * std::f64::consts::PI * t).cos()) / 2.0;
                let x = base_x + offset;

                for label in &popup_labels {
                    if let Some(win) = app.get_webview_window(label) {
                        if let Ok(pos) = win.outer_position() {
                            win.set_position(tauri::PhysicalPosition::new(x as i32, pos.y as i32)).ok();
                        }
                    }
                }
                std::thread::sleep(std::time::Duration::from_millis(16));
            }
        }
    });
}

// ─── HTTP Server for Python Monitor ───

const REMINDER_SERVER_PORT: u16 = 8766;

fn start_reminder_http_server(app: tauri::AppHandle) {
    use std::sync::Arc;

    std::thread::spawn(move || {
        use tiny_http::{Server, Response};
        use std::io::Read;

        let server = match Server::http(format!("127.0.0.1:{}", REMINDER_SERVER_PORT)) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("[reminder-http] Failed to start server: {}", e);
                return;
            }
        };
        println!("[reminder-http] Server running on port {}", REMINDER_SERVER_PORT);

        let app = Arc::new(app);

        for request in server.incoming_requests() {
            handle_reminder_request(&app, request);
        }
    });
}

fn handle_reminder_request(app: &Arc<tauri::AppHandle>, mut request: tiny_http::Request) {
    use tiny_http::{Method, Response};
    use std::io::Cursor;

    // Handle /slide-left: animate all todo popup windows left off-screen, then destroy them
    if request.method() == &Method::Post && request.url() == "/slide-left" {
        // Bounce generation — invalidates any stale 3s delay threads from previous context
        if let Some(gen) = app.try_state::<Arc<PopupBounceGen>>() {
            gen.gen.fetch_add(1, Ordering::SeqCst);
        }
        // Stop any running popup bounce and kill polling thread
        stop_popup_bounce(&app);
        if let Some(state) = app.try_state::<Arc<PopupExpanded>>() {
            state.expanded.store(false, Ordering::SeqCst);
            state.polling_active.store(false, Ordering::SeqCst);
            state.slide_in_progress.store(false, Ordering::SeqCst);
        }

        // Collect popup labels first so we don't hold the lock during animation
        let popup_labels: Vec<String> = app
            .webview_windows()
            .iter()
            .filter(|(label, _)| label.starts_with("todo-popup-"))
            .map(|(label, _)| label.to_string())
            .collect();

        for label in popup_labels {
            slide_left_todo_popup(&app, &label);
        }

        let resp_body = Cursor::new(b"{\"status\":\"sliding-left\"}");
        let _ = request.respond(Response::new(
            tiny_http::StatusCode(200),
            Vec::new(),
            resp_body,
            None,
            None,
        ));
        return;
    }

    // Handle /slide-left-popup: slide all popup windows back to x=-380 (context change / force hide)
    if request.method() == &Method::Post && request.url() == "/slide-left-popup" {
        // Bump slide gen to kill any running slide threads
        if let Some(gen) = app.try_state::<Arc<PopupSlideGen>>() {
            gen.gen.fetch_add(1, Ordering::SeqCst);
        }
        // Clear expanded flag, reset polling guard, and stop bounce to kill any running polling/bounce threads
        if let Some(state) = app.try_state::<Arc<PopupExpanded>>() {
            state.expanded.store(false, Ordering::SeqCst);
            state.polling_active.store(false, Ordering::SeqCst);
            state.slide_in_progress.store(false, Ordering::SeqCst);
        }
        stop_popup_bounce(&app);

        let popup_labels: Vec<String> = app
            .webview_windows()
            .iter()
            .filter(|(label, _)| label.starts_with("todo-popup-"))
            .map(|(label, _)| label.to_string())
            .collect();

        // Capture generation for cancellation
        let captured_gen = app.try_state::<Arc<PopupSlideGen>>()
            .map(|g| g.gen.load(Ordering::SeqCst));

        for label in &popup_labels {
            if let Some(win) = app.get_webview_window(label) {
                let from_x = match win.outer_position() {
                    Ok(pos) => pos.x as f64,
                    Err(_) => -20.0,
                };
                let label = label.clone();
                let app = app.clone();
                std::thread::spawn(move || {
                    let steps = 20;
                    let total_distance = -380.0 - from_x;
                    let step_delay = std::time::Duration::from_millis(15);
                    for i in 1..=steps {
                        // Abort if a newer slide command invalidated us
                        if let Some(gen) = captured_gen {
                            if let Some(state) = app.try_state::<Arc<PopupSlideGen>>() {
                                if state.gen.load(Ordering::SeqCst) != gen { return; }
                            }
                        }
                        let progress = i as f64 / steps as f64;
                        let eased = 1.0 - (1.0 - progress).powi(3);
                        let current_x = from_x + total_distance * eased;
                        if let Some(win) = app.get_webview_window(&label) {
                            if let Ok(pos) = win.outer_position() {
                                win.set_position(tauri::PhysicalPosition::new(current_x as i32, pos.y as i32)).ok();
                            }
                        }
                        std::thread::sleep(step_delay);
                    }
                });
            }
        }

        let resp_body = Cursor::new(b"{\"status\":\"sliding-left-popup\"}");
        let _ = request.respond(Response::new(
            tiny_http::StatusCode(200),
            Vec::new(),
            resp_body,
            None,
            None,
        ));
        return;
    }

/// Polling thread for popup hover: checks cursor position every 1s,
/// slides windows back to x=-380 if cursor leaves (with 300ms grace period).
fn poll_popup_hover(app: Arc<tauri::AppHandle>, popup_labels: Vec<String>) {
    // Wait for slide-right animation to complete before polling
    std::thread::sleep(std::time::Duration::from_millis(400));

    loop {
        // Check if still expanded
        let still_expanded = app.try_state::<Arc<PopupExpanded>>()
            .map(|s| s.expanded.load(Ordering::SeqCst))
            .unwrap_or(false);
        if !still_expanded {
            // Another handler cleared us — reset flag so next hover can spawn
            if let Some(state) = app.try_state::<Arc<PopupExpanded>>() {
                state.polling_active.store(false, Ordering::SeqCst);
            }
            return;
        }

        // Pause polling while a popup is being destroyed (checkbox click)
        let slide_busy = app.try_state::<Arc<PopupExpanded>>()
            .map(|s| s.slide_in_progress.load(Ordering::SeqCst))
            .unwrap_or(false);
        if slide_busy {
            std::thread::sleep(std::time::Duration::from_millis(100));
            continue;
        }

        // Get cursor position using Win32 API
        let cursor_in_popup = {
            #[repr(C)]
            struct POINT {
                x: i32,
                y: i32,
            }
            extern "system" {
                fn GetCursorPos(lpPoint: *mut POINT) -> i32;
            }
            let mut pt = POINT { x: 0, y: 0 };
            let ok = unsafe { GetCursorPos(&mut pt) };
            if ok == 0 {
                continue;
            }
            // Compute combined bounding box across all popups (includes gaps)
            let mut min_x = i32::MAX;
            let mut max_x = i32::MIN;
            let mut min_y = i32::MAX;
            let mut max_y = i32::MIN;
            for label in &popup_labels {
                if let Some(win) = app.get_webview_window(label) {
                    if let (Ok(pos), Ok(size)) = (win.outer_position(), win.inner_size()) {
                        let wx = pos.x as i32;
                        let wy = pos.y as i32;
                        let ww = size.width as i32;
                        let wh = size.height as i32;
                        if wx < min_x { min_x = wx; }
                        if wx + ww > max_x { max_x = wx + ww; }
                        if wy < min_y { min_y = wy; }
                        if wy + wh > max_y { max_y = wy + wh; }
                    }
                }
            }
            pt.x >= min_x && pt.x <= max_x && pt.y >= min_y && pt.y <= max_y
        };

        if !cursor_in_popup {
            // Mouse is outside — start 300ms grace period before sliding back.
            // If user re-enters during this window, /slide-right will set
            // expanded back to true, cancelling the slide.
            if let Some(state) = app.try_state::<Arc<PopupExpanded>>() {
                state.expanded.store(false, Ordering::SeqCst);
            }
            std::thread::sleep(std::time::Duration::from_millis(300));

            // If a checkbox click or context change happened during the grace period,
            // abort the slide-back to let the destruction/context change finish.
            let slide_busy = app.try_state::<Arc<PopupExpanded>>()
                .map(|s| s.slide_in_progress.load(Ordering::SeqCst))
                .unwrap_or(false);
            if slide_busy {
                continue;
            }

            // Failsafe: re-check actual cursor position after grace period.
            // Even if the frontend swallowed the mouseenter event (animLockRef debounce),
            // the physical cursor might be back over the popups — don't dismiss in that case.
            {
                #[repr(C)]
                struct POINT { x: i32, y: i32 }
                extern "system" { fn GetCursorPos(lpPoint: *mut POINT) -> i32; }
                let mut pt = POINT { x: 0, y: 0 };
                let ok = unsafe { GetCursorPos(&mut pt) };
                if ok != 0 {
                    let mut min_x = i32::MAX;
                    let mut max_x = i32::MIN;
                    let mut min_y = i32::MAX;
                    let mut max_y = i32::MIN;
                    for label in &popup_labels {
                        if let Some(win) = app.get_webview_window(label) {
                            if let (Ok(pos), Ok(size)) = (win.outer_position(), win.inner_size()) {
                                let wx = pos.x as i32;
                                let wy = pos.y as i32;
                                let ww = size.width as i32;
                                let wh = size.height as i32;
                                if wx < min_x { min_x = wx; }
                                if wx + ww > max_x { max_x = wx + ww; }
                                if wy < min_y { min_y = wy; }
                                if wy + wh > max_y { max_y = wy + wh; }
                            }
                        }
                    }
                    if pt.x >= min_x && pt.x <= max_x && pt.y >= min_y && pt.y <= max_y {
                        // Cursor physically returned — re-expand and keep polling
                        if let Some(state) = app.try_state::<Arc<PopupExpanded>>() {
                            state.expanded.store(true, Ordering::SeqCst);
                        }
                        continue;
                    }
                }
            }

            // Double-check the expanded flag (frontend might have sent /slide-right)
            let still_expanded = app.try_state::<Arc<PopupExpanded>>()
                .map(|s| s.expanded.load(Ordering::SeqCst))
                .unwrap_or(false);
            if still_expanded {
                continue; // Mouse came back, keep polling, don't slide
            }

            // Bump slide gen to kill any existing slide threads before spawning new ones
            if let Some(gen) = app.try_state::<Arc<PopupSlideGen>>() {
                gen.gen.fetch_add(1, Ordering::SeqCst);
            }
            let captured_gen = app.try_state::<Arc<PopupSlideGen>>()
                .map(|g| g.gen.load(Ordering::SeqCst));

            // Slide all popups back to -380
            for label in &popup_labels {
                if let Some(win) = app.get_webview_window(label) {
                    let from_x = match win.outer_position() {
                        Ok(pos) => pos.x as f64,
                        Err(_) => -20.0,
                    };
                    let label = label.clone();
                    let app = app.clone();
                    std::thread::spawn(move || {
                        let steps = 20;
                        let total_distance = -380.0 - from_x;
                        let step_delay = std::time::Duration::from_millis(15);
                        for i in 1..=steps {
                            // Abort if a newer slide command invalidated us
                            if let Some(gen) = captured_gen {
                                if let Some(state) = app.try_state::<Arc<PopupSlideGen>>() {
                                    if state.gen.load(Ordering::SeqCst) != gen { return; }
                                }
                            }
                            let progress = i as f64 / steps as f64;
                            let eased = 1.0 - (1.0 - progress).powi(3);
                            let current_x = from_x + total_distance * eased;
                            if let Some(win) = app.get_webview_window(&label) {
                                if let Ok(pos) = win.outer_position() {
                                    win.set_position(tauri::PhysicalPosition::new(current_x as i32, pos.y as i32)).ok();
                                }
                            }
                            std::thread::sleep(step_delay);
                        }
                    });
                }
            }
            break;
        }

        std::thread::sleep(std::time::Duration::from_millis(1000));
    }

    // Polling thread finished — reset flag so next hover can spawn a new one
    if let Some(state) = app.try_state::<Arc<PopupExpanded>>() {
        state.polling_active.store(false, Ordering::SeqCst);
    }
}

    // Handle /slide-right: slide all popup windows right to x=-20 (hover reveal)
    if request.method() == &Method::Post && request.url() == "/slide-right" {
        // Set expanded flag, clear any in-progress destroy slide
        if let Some(state) = app.try_state::<Arc<PopupExpanded>>() {
            state.expanded.store(true, Ordering::SeqCst);
            state.slide_in_progress.store(false, Ordering::SeqCst);
        }

        // Bump slide gen to kill any existing slide threads (slide-back, slide-left-popup)
        if let Some(gen) = app.try_state::<Arc<PopupSlideGen>>() {
            gen.gen.fetch_add(1, Ordering::SeqCst);
        }

        // Stop popup bounce
        stop_popup_bounce(&app);

        let popup_labels: Vec<String> = app
            .webview_windows()
            .iter()
            .filter(|(label, _)| label.starts_with("todo-popup-"))
            .map(|(label, _)| label.to_string())
            .collect();

        // Capture generation for cancellation
        let captured_gen = app.try_state::<Arc<PopupSlideGen>>()
            .map(|g| g.gen.load(Ordering::SeqCst));

        // Slide all popups to x=-20
        for label in &popup_labels {
            if let Some(win) = app.get_webview_window(label) {
                let from_x = match win.outer_position() {
                    Ok(pos) => pos.x as f64,
                    Err(_) => -380.0,
                };
                let label = label.clone();
                let app = app.clone();
                std::thread::spawn(move || {
                    let steps = 20;
                    let total_distance = -20.0 - from_x;
                    let step_delay = std::time::Duration::from_millis(15);
                    for i in 1..=steps {
                        // Abort if a newer slide command invalidated us
                        if let Some(gen) = captured_gen {
                            if let Some(state) = app.try_state::<Arc<PopupSlideGen>>() {
                                if state.gen.load(Ordering::SeqCst) != gen { return; }
                            }
                        }
                        let progress = i as f64 / steps as f64;
                        let eased = 1.0 - (1.0 - progress).powi(3);
                        let current_x = from_x + total_distance * eased;
                        if let Some(win) = app.get_webview_window(&label) {
                            if let Ok(pos) = win.outer_position() {
                                win.set_position(tauri::PhysicalPosition::new(current_x as i32, pos.y as i32)).ok();
                            }
                        }
                        std::thread::sleep(step_delay);
                    }
                });
            }
        }

        // Mark all visible popup todos as seen
        let todo_ids_to_mark: Vec<String> = popup_labels
            .iter()
            .filter_map(|l| l.strip_prefix("todo-popup-").map(|s| s.to_string()))
            .collect();

        if !todo_ids_to_mark.is_empty() {
            if let Ok(mut todos) = get_all_todos(&app) {
                let mut changed = false;
                for todo in &mut todos {
                    if todo_ids_to_mark.contains(&todo.id) && !todo.seen {
                        todo.seen = true;
                        changed = true;
                    }
                }
                if changed {
                    let _ = save_all_todos(&app, &todos);
                }
            }
        }

        // Spawn polling thread: every 1s check if cursor is over any popup
        // Guard: only one polling thread at a time to prevent races
        if let Some(state) = app.try_state::<Arc<PopupExpanded>>() {
            if state.polling_active.swap(true, Ordering::SeqCst) {
                // Another polling thread already running — skip spawn
            } else {
                let popup_labels_clone = popup_labels.clone();
                let app_clone = app.clone();
                std::thread::spawn(move || {
                    poll_popup_hover(app_clone, popup_labels_clone);
                });
            }
        }

        let resp_body = Cursor::new(b"{\"status\":\"sliding-right\"}");
        let _ = request.respond(Response::new(
            tiny_http::StatusCode(200),
            Vec::new(),
            resp_body,
            None,
            None,
        ));
        return;
    }

    // Only accept POST to /remind
    if request.method() != &Method::Post || request.url() != "/remind" {
        let body = Cursor::new(b"{}");
        let _ = request.respond(Response::new(
            tiny_http::StatusCode(404),
            Vec::new(),
            body,
            None,
            None,
        ));
        return;
    }

    // Read the request body
    let mut body = Vec::new();
    if request.as_reader().read_to_end(&mut body).is_err() {
        return;
    }

    // Parse the JSON payload
    let payload: serde_json::Value = match serde_json::from_slice(&body) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("[reminder-http] JSON parse error: {}", e);
            return;
        }
    };

    // Extract todos and spawn popup windows
    if let Some(todos) = payload["todos"].as_array() {
        let todos: Vec<serde_json::Value> = todos.iter().cloned().collect();
        let undone: Vec<&serde_json::Value> = todos.iter()
            .filter(|t| t.get("status").and_then(|s| s.as_str()) != Some("done"))
            .collect();
        if !undone.is_empty() {
            let popup_todos: Vec<TodoItem> = undone
                .iter()
                .filter_map(|t| serde_json::from_value::<TodoItem>((*t).clone()).ok())
                .collect();
            if !popup_todos.is_empty() {
                spawn_todo_popup_windows(&app, &popup_todos);
            }
        }
    }

    let resp_body = Cursor::new(b"{\"status\":\"ok\"}");
    let _ = request.respond(Response::new(
        tiny_http::StatusCode(200),
        Vec::new(),
        resp_body,
        None,
        None,
    ));
}

/// Debug: print a message from the frontend to the terminal.
#[tauri::command]
async fn popup_debug(msg: String) {
    println!("[popup-debug] {}", msg);
}

/// Register a popup's measured height and reposition all popups with correct gaps.
/// Called by each popup after it resizes itself. The last popup to report does the
/// final layout — no locking needed, each reposition is atomic and idempotent.
#[tauri::command]
async fn register_popup_height(app: tauri::AppHandle, label: String, height: f64) -> Result<(), String> {
    // Store this popup's height
    if let Some(state) = app.try_state::<Arc<PopupHeights>>() {
        state.map.lock().expect("poisoned").insert(label, height);
    }

    // Collect all popup labels and their current positions
    let mut popups: Vec<(String, f64)> = app
        .webview_windows()
        .iter()
        .filter(|(label, _)| label.starts_with("todo-popup-"))
        .filter_map(|(label, _)| {
            app.get_webview_window(label)
                .and_then(|w| w.outer_position().ok().map(|p| (label.to_string(), p.y as f64)))
        })
        .collect();

    // Sort by current Y position to preserve spawn order
    popups.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));

    // Read registered heights, fall back to current window size for unregistered popups
    let height_map = app.try_state::<Arc<PopupHeights>>()
        .map(|s| s.map.lock().expect("poisoned").clone())
        .unwrap_or_default();

    // Reposition with cumulative Y offsets
    let mut y = 50.0;
    const GAP: f64 = 10.0;
    let mut debug_parts = Vec::new();
    for (popup_label, _current_y) in &popups {
        let popup_height = height_map.get(popup_label)
            .copied()
            .unwrap_or_else(|| {
                // Fallback: read actual window height
                app.get_webview_window(popup_label)
                    .and_then(|w| w.outer_size().ok().map(|s| s.height as f64))
                    .unwrap_or(31.0)
            });

        if let Some(win) = app.get_webview_window(popup_label) {
            if let Ok(pos) = win.outer_position() {
                win.set_position(tauri::PhysicalPosition::new(pos.x as i32, y as i32)).ok();
            }
        }

        // Debug: label + registered height + computed Y + bottom edge
        let registered = height_map.get(popup_label).map(|h| *h as i32).unwrap_or(-1);
        debug_parts.push(format!("{}(reg={}→y={},bottom={})", popup_label, registered, y as i32, (y + popup_height) as i32));

        y += popup_height + GAP;
    }

    println!("[popup-layout] {}", debug_parts.join(" | "));

    Ok(())
}

/// Tauri command: lock a popup for destruction so the polling thread yields immediately.
/// Call this BEFORE the CSS fade delay so the polling thread doesn't race into a slide-back.
#[tauri::command]
async fn lock_popup_for_destruction(app: tauri::AppHandle, label: String) -> Result<(), String> {
    if app.get_webview_window(&label).is_some() {
        if let Some(state) = app.try_state::<Arc<PopupExpanded>>() {
            state.slide_in_progress.store(true, Ordering::SeqCst);
        }
    }
    Ok(())
}

/// Tauri command: slide a single todo popup window left off-screen and destroy it.
#[tauri::command]
async fn slide_left_and_destroy_popup(app: tauri::AppHandle, label: String) -> Result<(), String> {
    if let Some(win) = app.get_webview_window(&label) {
        let from_x = match win.outer_position() {
            Ok(pos) => pos.x as f64,
            Err(_) => -20.0,
        };

        // Bump slide gen to kill any existing slide threads (slide-back)
        if let Some(gen) = app.try_state::<Arc<PopupSlideGen>>() {
            gen.gen.fetch_add(1, Ordering::SeqCst);
        }
        let captured_gen = app.try_state::<Arc<PopupSlideGen>>()
            .map(|g| g.gen.load(Ordering::SeqCst));

        // Animate left in background thread
        let app_clone = app.clone();
        std::thread::spawn(move || {
            let steps = 20;
            let target_x = -400.0;
            let total_distance = target_x - from_x;
            let step_delay = std::time::Duration::from_millis(15);
            for i in 1..=steps {
                // Abort if a newer slide command invalidated us
                if let Some(gen) = captured_gen {
                    if let Some(state) = app_clone.try_state::<Arc<PopupSlideGen>>() {
                        if state.gen.load(Ordering::SeqCst) != gen {
                            // Still need to resume polling
                            if let Some(state) = app_clone.try_state::<Arc<PopupExpanded>>() {
                                state.slide_in_progress.store(false, Ordering::SeqCst);
                            }
                            return;
                        }
                    }
                }
                let progress = i as f64 / steps as f64;
                let eased = 1.0 - (1.0 - progress).powi(3);
                let current_x = from_x + total_distance * eased;
                if let Some(win) = app.get_webview_window(&label) {
                    if let Ok(pos) = win.outer_position() {
                        win.set_position(tauri::PhysicalPosition::new(current_x as i32, pos.y as i32)).ok();
                    }
                }
                std::thread::sleep(step_delay);
            }
            // Destroy after animation
            if let Some(win) = app.get_webview_window(&label) {
                win.destroy().ok();
            }
            // Resume polling
            if let Some(state) = app_clone.try_state::<Arc<PopupExpanded>>() {
                state.slide_in_progress.store(false, Ordering::SeqCst);
            }
        });
    }
    Ok(())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_store::Builder::default().build())
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .on_window_event(|_window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
                _window.hide().ok();
            }
        })
        .setup(|app| {
            // --- Initialize store ---
            let store = app.store("notes.json")?;
            if store.get("notes").is_none() {
                store.set("notes", serde_json::json!([]));
                store.save()?;
            }

            // Initialize todos store
            let todos_store = app.store("todos.json")?;
            if todos_store.get("todos").is_none() {
                todos_store.set("todos", serde_json::json!([]));
                todos_store.save()?;
            }

            // Initialize contexts store
            let ctx_store = app.store("contexts.json")?;
            if ctx_store.get("contexts").is_none() {
                ctx_store.set("contexts", serde_json::json!({}));
                ctx_store.save()?;
            }

            // Spawn the Python context classifier server
            spawn_context_server();

            // --- Start HTTP server for Python monitor ---
            start_reminder_http_server(app.handle().clone());

            // Check if notes exist and are non-empty; create a default note if not.
            let needs_default = match store.get("notes") {
                None => true,
                Some(v) => v.as_array().map_or(true, |a| a.is_empty()),
            };

            if needs_default {
                let default_note = Note {
                    id: Uuid::new_v4().to_string(),
                    title: "ToDo".to_string(),
                    x: 100.0,
                    y: 100.0,
                    width: 300.0,
                    height: 200.0,
                    color: "#FFE066".to_string(),
                    todo_ids: Vec::new(),
                };
                store.set("notes", serde_json::json!([default_note]));
                store.save()?;
                // Spawn the default note window immediately
                spawn_note_window(&app.handle(), &default_note);
            }

            // --- Restore notes on launch ---
            spawn_notes_on_launch(app.handle()).ok();

            // --- Register global shortcut ---
            let default_hotkey =
                Shortcut::new(Some(Modifiers::CONTROL | Modifiers::SHIFT), Code::KeyS);

            let hotkey = {
                let store = app.store("notes.json")?;
                store
                    .get("hotkey")
                    .and_then(|v| v.as_str().map(|s| s.to_string()))
                    .and_then(|s| parse_hotkey_string(&s))
                    .unwrap_or(default_hotkey)
            };

            let tracked_shortcut = hotkey.clone();
            app.global_shortcut()
                .on_shortcut(hotkey.clone(), |app_handle, _shortcut, event| {
                    if event.state == ShortcutState::Pressed {
                        toggle_note_windows(app_handle);
                    }
                })
                .map_err(|e| e.to_string())?;

            // Store the initial shortcut in state for later re-registration
            app.manage(GlobalShortcutState {
                current: Mutex::new(Some(tracked_shortcut)),
            });

            // Initialize notes visibility tracking (true = notes are shown)
            app.manage(NotesVisibility {
                notes_visible: Mutex::new(true),
            });

            // Track popup bounce active flag
            app.manage(Arc::new(PopupBounceActive { active: AtomicBool::new(false) }));

            // Track popup bounce generation (cancels stale delayed bounce threads)
            app.manage(Arc::new(PopupBounceGen { gen: AtomicU32::new(0) }));

            // Track whether popup windows are expanded (for hover polling)
            app.manage(Arc::new(PopupExpanded {
                expanded: AtomicBool::new(false),
                polling_active: AtomicBool::new(false),
                slide_in_progress: AtomicBool::new(false),
            }));

            // Track measured popup heights for dynamic Y positioning
            app.manage(Arc::new(PopupHeights {
                map: Mutex::new(HashMap::new()),
            }));

            // Track popup slide animation generation (cancels stale slide threads)
            app.manage(Arc::new(PopupSlideGen { gen: AtomicU32::new(0) }));

            // --- Tray icon & menu ---
            let menu = Menu::with_items(
                app,
                &[
                    &MenuItemBuilder::with_id("show_notes", "Show Notes").build(app)?,
                    &MenuItemBuilder::with_id("preferences", "Preferences").build(app)?,
                    &PredefinedMenuItem::separator(app)?,
                    &PredefinedMenuItem::quit(app, None)?,
                ],
            )?;

            TrayIconBuilder::with_id("tray")
                .icon(app.default_window_icon().unwrap().clone())
                .tooltip("Sticky Notes")
                .menu(&menu)
                .show_menu_on_left_click(true)
                .on_tray_icon_event(|tray, event| {
                    if let TrayIconEvent::Click {
                        button: MouseButton::Left,
                        ..
                    } = event
                    {
                        toggle_note_windows(tray.app_handle());
                    }
                })
                .build(app)?;

            // --- Tray menu event handler ---
            app.on_menu_event(|app, event| {
                match event.id().as_ref() {
                    "show_notes" => {
                        // Toggle notes via the same logic as the hotkey
                        toggle_note_windows(app);
                    }
                    "preferences" => {
                        let app = app.clone();
                        tauri::async_runtime::spawn(async move {
                            let _ = spawn_preferences_window(app).await;
                        });
                    }
                    "quit" => {
                        app.exit(0);
                    }
                    _ => {}
                }
            });

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            spawn_preferences_window,
            create_note,
            re_register_shortcut,
            note_hidden,
            add_todo,
            toggle_todo,
            delete_todo,
            delete_note_todos,
            get_note_todos,
            register_popup_height,
            lock_popup_for_destruction,
            slide_left_and_destroy_popup,
            popup_debug
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
