import { useState, useEffect, useRef } from "react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { invoke } from "@tauri-apps/api/core";
import "../styles/reminder.css";

interface ReminderTodo {
  id: string;
  task: string;
  status: string;
}

export default function ReminderWindow() {
  const [todos, setTodos] = useState<ReminderTodo[]>([]);
  const [currentContext, setCurrentContext] = useState<string[]>([]);
  // Track todos that are fading out (strikethrough + opacity transition)
  const [fadingIds, setFadingIds] = useState<Set<string>>(new Set());
  // Debounce guard: ignore hover events during slide animation (~300ms + safety margin)
  const animLockRef = useRef(false);

  // Listen for reminder data from Rust
  useEffect(() => {
    const unlisten = getCurrentWindow().listen<{ todos: ReminderTodo[]; context: string }>(
      "reminder-data",
      (event) => {
        const data = event.payload;
        setFadingIds(new Set());
        const undone = data.todos.filter((t) => t.status !== "done");
        setTodos(undone);
        if (data.context) {
          setCurrentContext(data.context.split(", "));
        }
      }
    );

    return () => {
      unlisten.then((f) => f());
    };
  }, []);

  const handleToggle = async (todoId: string) => {
    try {
      await invoke("toggle_todo", { todoId });
      // Mark as fading (triggers CSS strikethrough + fade animation)
      setFadingIds((prev) => new Set([...prev, todoId]));
      // Remove from list after animation completes
      setTimeout(() => {
        setTodos((prev) => {
          const newTodos = prev.filter((t) => t.id !== todoId);
          // If list is now empty, check backend for remaining undone todos in current context
          if (newTodos.length === 0) {
            invoke("check_context_todos_and_slide", { contexts: currentContext }).catch(() => {});
          }
          return newTodos;
        });
        setFadingIds((prev) => {
          const next = new Set(prev);
          next.delete(todoId);
          return next;
        });
      }, 600);
    } catch (err) {
      console.error("Failed to toggle todo:", err);
    }
  };

  const handleMouseEnter = () => {
    if (animLockRef.current) return;
    animLockRef.current = true;
    fetch("http://127.0.0.1:8766/slide-down", { method: "POST" }).catch(() => {});
    setTimeout(() => { animLockRef.current = false; }, 400);
  };

  const handleMouseLeave = () => {
    if (animLockRef.current) return;
    animLockRef.current = true;
    fetch("http://127.0.0.1:8766/slide-up", { method: "POST" }).catch(() => {});
    setTimeout(() => { animLockRef.current = false; }, 400);
  };

  return (
    <div className="reminder-window" onMouseEnter={handleMouseEnter} onMouseLeave={handleMouseLeave}>
      {todos.length > 0 ? (
        todos.map((todo) => (
          <div
            key={todo.id}
            className={`reminder-todo-item${fadingIds.has(todo.id) ? " fading" : ""}`}
          >
            <input
              type="checkbox"
              checked={fadingIds.has(todo.id)}
              onChange={() => handleToggle(todo.id)}
            />
            <span className="reminder-todo-text">{todo.task}</span>
          </div>
        ))
      ) : (
        <div className="reminder-placeholder">No todos for this window...</div>
      )}
    </div>
  );
}
