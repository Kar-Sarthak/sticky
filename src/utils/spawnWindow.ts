import { WebviewWindow } from "@tauri-apps/api/webviewWindow";
import type { Note } from "../types";

/**
 * Creates a new note WebviewWindow or focuses it if it already exists.
 * Used when the frontend needs to spawn a note window directly.
 */
export async function spawnNoteWindow(note: Note): Promise<WebviewWindow | null> {
  const label = `note-${note.id}`;

  // Check if window already exists (returns Promise in Tauri 2)
  const existing = await WebviewWindow.getByLabel(label);
  if (existing) {
    await existing.show();
    await existing.setFocus();
    return existing;
  }

  try {
    const webview = new WebviewWindow(label, {
      title: note.title || "Note",
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
  } catch (err) {
    console.error("Failed to create note window:", err);
    return null;
  }
}

/**
 * Creates the floating + button window at the bottom-right of the screen.
 */
export async function spawnAddButtonWindow(): Promise<WebviewWindow | null> {
  const label = "add-button";
  const existing = await WebviewWindow.getByLabel(label);
  if (existing) {
    await existing.show();
    await existing.setFocus();
    return existing;
  }

  try {
    const webview = new WebviewWindow(label, {
      url: "index.html#add-button",
      title: "Add Note",
      width: 60,
      height: 60,
      resizable: false,
      decorations: false,
      transparent: true,
      alwaysOnTop: true,
      skipTaskbar: true,
    });

    return webview;
  } catch (err) {
    console.error("Failed to create add-button window:", err);
    return null;
  }
}
