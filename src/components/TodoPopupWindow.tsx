import { useState, useEffect, useRef } from "react";
import { getCurrentWindow, LogicalSize } from "@tauri-apps/api/window";
import { invoke } from "@tauri-apps/api/core";
import "../styles/todo-popup.css";

export default function TodoPopupWindow() {
  const [task, setTask] = useState("");
  const [todoId, setTodoId] = useState("");
  const [done, setDone] = useState(false);
  const animLockRef = useRef(false);
  const containerRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    const params = new URLSearchParams(window.location.hash.split("?")[1] || "");
    const taskParam = params.get("task") || "";
    const idParam = params.get("id") || "";
    setTask(decodeURIComponent(taskParam));
    setTodoId(idParam);
  }, []);

  // Measure wrapped content and resize the window to fit
  useEffect(() => {
    if (!task) return;
    const el = containerRef.current;
    if (!el) return;

    // Wait for the font to load so the text wraps correctly before measuring
    document.fonts.ready.then(() => {
      const textEl = el.querySelector(".todo-popup-text") as HTMLElement | null;
      const textScroll = textEl?.scrollHeight;

      // Compute total height from text content + item padding (4px top + 4px bottom) + border
      const itemHeight = (textScroll || 0) + 9;
      const win = getCurrentWindow();
      win.setSize(new LogicalSize(300, itemHeight))
        .then(async () => {
          // Read actual outer size after resize
          const outer = await win.outerSize();
          invoke("popup_debug", {
            msg: `label=${win.label} itemHeight=${itemHeight} outerHeight=${outer.height} textScroll=${textScroll}`,
          }).catch(() => {});
          // Register height so Rust can reposition all popups with correct gaps
          invoke("register_popup_height", { label: win.label, height: outer.height }).catch(() => {});
        })
        .catch((e) => invoke("popup_debug", { msg: `label=${win.label} setSize FAILED → 300x${itemHeight} err=${e}` }));
    });
  }, [task]);

  const handleToggle = async () => {
    if (!todoId || done) return;
    try {
      const win = getCurrentWindow();
      await invoke("toggle_todo", { todoId });
      setDone(true);

      // Lock immediately so the polling thread yields during the fade delay.
      // Without this, the 500ms gap creates a race: cursor leaves → polling
      // thread triggers slide-back → fights with the destroy thread.
      await invoke("lock_popup_for_destruction", { label: win.label });

      setTimeout(async () => {
        await invoke("slide_left_and_destroy_popup", { label: win.label });
      }, 500);
    } catch (err) {
      console.error("Failed to toggle todo:", err);
    }
  };

  const handleMouseEnter = () => {
    if (animLockRef.current) return;
    animLockRef.current = true;
    fetch("http://127.0.0.1:8766/slide-right", { method: "POST" }).catch(() => {});
    setTimeout(() => { animLockRef.current = false; }, 400);
  };

  const handleMouseLeave = () => {
    if (animLockRef.current) return;
    animLockRef.current = true;
    // Polling thread on Rust side handles slide-back
    setTimeout(() => { animLockRef.current = false; }, 400);
  };

  return (
    <div className="todo-popup-window" ref={containerRef} onMouseEnter={handleMouseEnter} onMouseLeave={handleMouseLeave}>
      <div className={`todo-popup-item${done ? " done" : ""}`}>
        <input type="checkbox" checked={done} onChange={handleToggle} />
        <span className="todo-popup-text">{task}</span>
      </div>
    </div>
  );
}
