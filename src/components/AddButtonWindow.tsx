import { invoke } from "@tauri-apps/api/core";

/**
 * Phase 6: Floating + button pinned to the bottom-right of the screen.
 * Clicking it creates a new note via the `create_note` Rust command.
 */
export default function AddButtonWindow() {
  const handleClick = async () => {
    try {
      await invoke("create_note");
    } catch (err) {
      console.error("Failed to create note:", err);
    }
  };

  return (
    <div className="add-button" onClick={handleClick} title="Add Note">
      +
    </div>
  );
}
