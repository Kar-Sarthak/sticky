import { useState, useEffect, useRef } from "react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { invoke } from "@tauri-apps/api/core";
import "../styles/todo-popup.css";

export default function TodoPopupWindow() {
  const [task, setTask] = useState("");
  const [todoId, setTodoId] = useState("");
  const [done, setDone] = useState(false);
  const animLockRef = useRef(false);

  useEffect(() => {
    const params = new URLSearchParams(window.location.hash.split("?")[1] || "");
    const taskParam = params.get("task") || "";
    const idParam = params.get("id") || "";
    setTask(decodeURIComponent(taskParam));
    setTodoId(idParam);
  }, []);

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
    <div className="todo-popup-window" onMouseEnter={handleMouseEnter} onMouseLeave={handleMouseLeave}>
      <div className={`todo-popup-item${done ? " done" : ""}`}>
        <input type="checkbox" checked={done} onChange={handleToggle} />
        <span className="todo-popup-text">{task}</span>
      </div>
    </div>
  );
}
