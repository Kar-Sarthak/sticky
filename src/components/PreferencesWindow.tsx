import { useState, useEffect, useCallback, useRef } from "react";
import { invoke } from "@tauri-apps/api/core";
import { getStore } from "../utils/store";

const DEFAULT_HOTKEY = "CommandOrControl+Shift+S";

export default function PreferencesWindow() {
  const [hotkey, setHotkey] = useState(DEFAULT_HOTKEY);
  const [saved, setSaved] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [isListening, setIsListening] = useState(false);

  const captureRef = useRef<HTMLDivElement>(null);

  // Load saved hotkey from store on mount
  useEffect(() => {
    (async () => {
      try {
        const store = await getStore();
        const savedKey = await store.get<string>("hotkey");
        if (savedKey) {
          setHotkey(savedKey);
        }
      } catch {
        // Store not yet available
      }
    })();
  }, []);

  const handleKeyDown = useCallback((e: React.KeyboardEvent) => {
    e.preventDefault();
    e.stopPropagation();

    const modifiers: string[] = [];
    if (e.ctrlKey) modifiers.push("CommandOrControl");
    if (e.altKey) modifiers.push("Alt");
    if (e.shiftKey) modifiers.push("Shift");

    // Skip modifier-only keys — wait for an actual key
    const skipKeys = new Set(["Control", "Alt", "Shift", "Meta"]);
    if (skipKeys.has(e.key)) return;

    // Build the key part
    let key = e.key;
    if (key.length === 1) {
      key = key.toUpperCase();
    } else if (key === " ") {
      key = "Space";
    } else if (key.startsWith("Arrow")) {
      key = key; // ArrowUp, ArrowDown, etc.
    } else if (key === "Backspace") {
      key = "Backspace";
    } else if (key === "Delete") {
      key = "Delete";
    } else if (key === "Enter") {
      key = "Enter";
    } else if (key === "Escape") {
      key = "Escape";
    } else if (key === "Tab") {
      key = "Tab";
    } else if (key.startsWith("F") && key.length <= 3) {
      // F1-F12
      key = key;
    } else {
      return; // Unsupported key
    }

    // Must have at least one modifier
    if (modifiers.length === 0) return;

    const newHotkey = [...modifiers, key].join("+");
    setHotkey(newHotkey);
    setIsListening(false);
  }, []);

  const toggleListening = useCallback(() => {
    setIsListening((prev) => !prev);
  }, []);

  const handleSave = useCallback(async () => {
    try {
      setError(null);
      await invoke("re_register_shortcut", { newHotkey: hotkey });
      setSaved(true);
      setTimeout(() => setSaved(false), 2000);
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    }
  }, [hotkey]);

  return (
    <div className="preferences">
      <h2>Preferences</h2>

      <div className="pref-section">
        <label className="pref-label">Global Shortcut</label>

        <div
          ref={captureRef}
          className={`hotkey-capture ${isListening ? "listening" : ""}`}
          onClick={toggleListening}
          onKeyDown={handleKeyDown}
          tabIndex={0}
          role="button"
        >
          {isListening ? (
            <span className="hotkey-placeholder">Press a shortcut...</span>
          ) : (
            <span className="hotkey-display">{hotkey || "No shortcut set"}</span>
          )}
          <span className="hotkey-icon">{isListening ? "⏎" : "⌨"}</span>
        </div>

        <p className="hint">Click to capture a new shortcut. Must include at least one modifier (Ctrl, Alt, Shift).</p>
      </div>

      <div className="pref-actions">
        <button onClick={handleSave} disabled={!hotkey}>
          Save
        </button>
        {saved && <span className="save-confirm">Saved!</span>}
        {error && <span className="save-error">{error}</span>}
      </div>
    </div>
  );
}
