import { useState, useEffect, useRef } from "react";
import "../styles/todo-popup.css";

export default function TodoPopupWindow() {
  const [task, setTask] = useState("");
  const animLockRef = useRef(false);

  useEffect(() => {
    const params = new URLSearchParams(window.location.hash.split("?")[1] || "");
    const taskParam = params.get("task") || "";
    setTask(decodeURIComponent(taskParam));
  }, []);

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
      <div className="todo-popup-item">
        <input type="checkbox" disabled />
        <span className="todo-popup-text">{task}</span>
      </div>
    </div>
  );
}
