import { useState, useEffect, useCallback } from "react";
import { invoke } from "@tauri-apps/api/core";
import { getStore } from "../utils/store";

const DEFAULT_HOTKEY = "CommandOrControl+Shift+S";

export default function PreferencesWindow() {
  const [hotkey, setHotkey] = useState(DEFAULT_HOTKEY);
  const [saved, setSaved] = useState(false);
  const [error, setError] = useState<string | null>(null);

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

      <div className="pref-field">
        <label htmlFor="hotkey-input">Global Shortcut</label>
        <input
          id="hotkey-input"
          type="text"
          value={hotkey}
          onChange={(e) => setHotkey(e.target.value)}
          placeholder="e.g. CommandOrControl+Shift+S"
        />
        <p className="hint">
          Common modifiers: <code>CommandOrControl</code>, <code>Shift</code>,{" "}
          <code>Alt</code>, <code>Super</code>
        </p>
      </div>

      <div className="pref-actions">
        <button onClick={handleSave}>Save</button>
        {saved && <span className="save-confirm">Saved!</span>}
        {error && <span className="save-error">{error}</span>}
      </div>
    </div>
  );
}
