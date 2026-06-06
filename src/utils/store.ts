import { Store } from "@tauri-apps/plugin-store";

let store: Store | null = null;

/**
 * Lazily loads the notes store. Subsequent calls return the cached instance.
 */
export async function getStore(): Promise<Store> {
  if (!store) {
    store = await Store.load("notes.json");
  }
  return store;
}

/**
 * Returns the full notes array from the store.
 */
export async function getNotes(): Promise<any[]> {
  const s = await getStore();
  return ((await s.get<any[]>("notes")) ?? []);
}

/**
 * Replaces the entire notes array in the store.
 */
export async function saveNotes(notes: any[]): Promise<void> {
  const s = await getStore();
  await s.set("notes", notes);
  await s.save();
}

/**
 * Appends a single note to the store.
 */
export async function addNote(note: any): Promise<void> {
  const notes = await getNotes();
  notes.push(note);
  await saveNotes(notes);
}

/**
 * Merges updates into an existing note by id.
 */
export async function updateNote(id: string, updates: Partial<any>): Promise<void> {
  const notes = await getNotes();
  const idx = notes.findIndex((n: any) => n.id === id);
  if (idx !== -1) {
    notes[idx] = { ...notes[idx], ...updates };
    await saveNotes(notes);
  }
}

/**
 * Removes a note from the store by id.
 */
export async function deleteNote(id: string): Promise<void> {
  const notes = await getNotes();
  await saveNotes(notes.filter((n: any) => n.id !== id));
}
