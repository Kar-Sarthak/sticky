import { useState, useEffect } from "react";
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
  // Track todos that are fading out (strikethrough + opacity transition)
  const [fadingIds, setFadingIds] = useState<Set<string>>(new Set());

  // Listen for reminder data from Rust
  useEffect(() => {
    const unlisten = getCurrentWindow().listen<{ todos: ReminderTodo[] }>(
      "reminder-data",
      (event) => {
        const data = event.payload;
        setFadingIds(new Set());
        const undone = data.todos.filter((t) => t.status !== "done");
        setTodos(undone);
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
        setTodos((prev) => prev.filter((t) => t.id !== todoId));
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

  return (
    <div className="reminder-window">
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
