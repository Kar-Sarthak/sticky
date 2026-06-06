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

  // Listen for reminder data from Rust
  useEffect(() => {
    const unlisten = getCurrentWindow().listen<{ todos: ReminderTodo[] }>(
      "reminder-data",
      (event) => {
        const data = event.payload;
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
      setTodos((prev) => prev.filter((t) => t.id !== todoId));
    } catch (err) {
      console.error("Failed to toggle todo:", err);
    }
  };

  return (
    <div className="reminder-window">
      {todos.length > 0 ? (
        todos.map((todo) => (
          <div key={todo.id} className="reminder-todo-item">
            <button
              className="reminder-checkbox"
              onClick={() => handleToggle(todo.id)}
              title="Mark as done"
            >
              {todo.status === "done" ? "✅" : "☐"}
            </button>
            <span className="reminder-todo-text">{todo.task}</span>
          </div>
        ))
      ) : (
        <div className="reminder-placeholder">No todos for this window...</div>
      )}
    </div>
  );
}
