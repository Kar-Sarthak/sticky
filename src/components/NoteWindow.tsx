import { useState, useEffect, useRef, useCallback } from "react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { invoke } from "@tauri-apps/api/core";
import { getNotes, updateNote, deleteNote } from "../utils/store";
import type { TodoItem } from "../types";
import "../styles/note.css";

/**
 * Full note UI — todo list with checkboxes, hover-reveal delete,
 * new todo input, drag grip, color picker.
 */
export default function NoteWindow() {
  const [note, setNote] = useState<{
    id: string;
    title: string;
    color: string;
  } | null>(null);

  const [todos, setTodos] = useState<TodoItem[]>([]);
  const [noteCount, setNoteCount] = useState(1);
  const [newTodoText, setNewTodoText] = useState("");
  const titleRef = useRef<HTMLSpanElement>(null);
  const newTodoRef = useRef<HTMLInputElement>(null);
  const [showTopFade, setShowTopFade] = useState(false);
  const [showBottomFade, setShowBottomFade] = useState(false);
  const [editingTitle, setEditingTitle] = useState(false);
  const [showColorPicker, setShowColorPicker] = useState(false);

  // --- Load note data and todos from store ---
  useEffect(() => {
    const hash = window.location.hash;
    const noteId = hash.replace("#note-", "");

    getNotes().then((notes) => {
      const found = notes.find((n) => n.id === noteId);
      if (found) {
        setNote({
          id: found.id,
          title: found.title,
          color: found.color,
        });
      }
      setNoteCount(notes.length);
    });

    // Load todos for this note
    const loadTodos = () => {
      invoke<TodoItem[]>("get_note_todos", { noteId })
        .then(setTodos)
        .catch(console.error);
    };
    loadTodos();

    // Listen for todo updates from other windows
    const unlistenTodoUpdated = getCurrentWindow().listen("todo-updated", () => {
      loadTodos();
    });

    return () => {
      unlistenTodoUpdated.then((f) => f());
    };
  }, []);

  // --- Poll note count so the delete button reflects current state ---
  useEffect(() => {
    const refreshCount = () => {
      getNotes().then((notes) => setNoteCount(notes.length));
    };
    refreshCount();
    const timer = setInterval(refreshCount, 500);
    return () => clearInterval(timer);
  }, []);

  // --- Check scroll position for fade indicators ---
  useEffect(() => {
    const container = document.querySelector(".note-todo-list");
    if (!container) return;

    const checkScroll = () => {
      setShowTopFade(container.scrollTop > 0);
      setShowBottomFade(
        container.scrollHeight > container.scrollTop + container.clientHeight + 2
      );
    };

    checkScroll();
    container.addEventListener("scroll", checkScroll);
    return () => container.removeEventListener("scroll", checkScroll);
  }, [todos]);

  // --- Sync position & size back to store ---
  useEffect(() => {
    if (!note) return;
    const win = getCurrentWindow();

    const unlistenMove = win.onMoved(({ payload }) => {
      updateNote(note.id, { x: payload.x, y: payload.y });
    });

    const unlistenResize = win.onResized(({ payload }) => {
      updateNote(note.id, { width: payload.width, height: payload.height });
    });

    return () => {
      unlistenMove.then((f) => f());
      unlistenResize.then((f) => f());
    };
  }, [note]);

  // --- Title: save on blur or Enter ---
  const handleTitleBlur = useCallback(() => {
    setEditingTitle(false);
    if (!note) return;
    const newTitle = titleRef.current?.textContent?.trim() || "ToDo";
    if (newTitle !== note.title) {
      setNote((prev) => (prev ? { ...prev, title: newTitle } : null));
      updateNote(note.id, { title: newTitle });
    }
  }, [note]);

  const handleTitleKeyDown = useCallback(
    (e: React.KeyboardEvent) => {
      if (e.key === "Enter") {
        e.preventDefault();
        handleTitleBlur();
      }
    },
    [handleTitleBlur]
  );

  // --- Click title to start editing ---
  const handleTitleMouseDown = useCallback(() => {
    if (editingTitle) return;
    titleRef.current?.setAttribute("contenteditable", "true");
    setEditingTitle(true);
  }, [editingTitle]);

  // Focus + select all after the re-render
  useEffect(() => {
    if (editingTitle && titleRef.current) {
      titleRef.current.focus();
      const range = document.createRange();
      range.selectNodeContents(titleRef.current);
      const sel = window.getSelection();
      sel?.removeAllRanges();
      sel?.addRange(range);
    }
  }, [editingTitle]);

  // --- Color change ---
  const handleColorChange = useCallback(
    (color: string) => {
      if (!note) return;
      setNote((prev) => (prev ? { ...prev, color } : null));
      updateNote(note.id, { color });
      setShowColorPicker(false);
    },
    [note]
  );

  // --- Drag via grip handle ---
  const handleDragMouseDown = useCallback(async (e: React.MouseEvent) => {
    e.preventDefault();
    const win = getCurrentWindow();
    await win.startDragging();
  }, []);

  // --- Close (hide this window only) and Delete ---
  const handleClose = async () => {
    await getCurrentWindow().hide();
    await invoke("note_hidden", { isDestroying: false });
  };

  const handleDelete = async () => {
    if (!note || noteCount <= 1) return;
    // First delete all todos belonging to this note
    await invoke("delete_note_todos", { noteId: note.id });
    await invoke("note_hidden", { isDestroying: true });
    await deleteNote(note.id);
    await getCurrentWindow().destroy();
  };

  // --- Create a new note ---
  const handleNewNote = async () => {
    try {
      await invoke("create_note");
    } catch (err) {
      console.error("Failed to create note:", err);
    }
  };

  // --- Add a new todo ---
  const handleAddTodo = useCallback(async () => {
    if (!note || !newTodoText.trim()) return;
    const task = newTodoText.trim();
    setNewTodoText("");

    try {
      const newTodo = await invoke<TodoItem>("add_todo", {
        noteId: note.id,
        task,
      });
      setTodos((prev) => [...prev, newTodo]);
      // Focus back on the input for quick entry
      newTodoRef.current?.focus();
    } catch (err) {
      console.error("Failed to add todo:", err);
      setNewTodoText(task);
    }
  }, [note, newTodoText]);

  // --- Toggle todo status ---
  const handleToggleTodo = useCallback(async (todoId: string) => {
    try {
      await invoke("toggle_todo", { todoId });
      setTodos((prev) =>
        prev.map((t) =>
          t.id === todoId
            ? { ...t, status: t.status === "done" ? "undone" : "done" }
            : t
        )
      );
    } catch (err) {
      console.error("Failed to toggle todo:", err);
    }
  }, []);

  // --- Delete a todo ---
  const handleDeleteTodo = useCallback(async (todoId: string) => {
    try {
      await invoke("delete_todo", { todoId });
      setTodos((prev) => prev.filter((t) => t.id !== todoId));
    } catch (err) {
      console.error("Failed to delete todo:", err);
    }
  }, []);

  if (!note) {
    return <div className="note-loading">Loading note...</div>;
  }

  const canDelete = noteCount > 1;

  return (
    <div className="note-container">
      <div className="note-inner" style={{ background: note.color }}>
        <header className="note-header">
          {/* Drag grip — left */}
          <div className="note-grip" onMouseDown={handleDragMouseDown}>
            <div className="grip-column">
              <span className="grip-dot" />
              <span className="grip-dot" />
              <span className="grip-dot" />
            </div>
            <div className="grip-column">
              <span className="grip-dot" />
              <span className="grip-dot" />
              <span className="grip-dot" />
            </div>
          </div>

          {/* Color picker button */}
          <div className="color-picker-wrapper">
            <button
              className="btn-action btn-color-picker"
              onClick={() => setShowColorPicker((v) => !v)}
              title="Change color"
            >
              <span
                className="color-dot"
                style={{
                  backgroundColor: note.color,
                  border: `2px solid rgba(0,0,0,0.15)`,
                }}
              />
            </button>
            {showColorPicker && (
              <div className="color-swatches" style={{ backgroundColor: note.color }}>
                {COLORS.map((c) => (
                  <button
                    key={c.hex}
                    className={`color-swatch${c.hex === note.color ? " color-swatch-active" : ""}`}
                    style={{ backgroundColor: c.hex }}
                    onClick={() => handleColorChange(c.hex)}
                    title={c.name}
                  />
                ))}
              </div>
            )}
          </div>

          {/* Title — hidden when color picker is open */}
          {!showColorPicker && (
            <span
              ref={titleRef}
              className="note-title"
              contentEditable={editingTitle}
              suppressContentEditableWarning
              onBlur={handleTitleBlur}
              onMouseDown={handleTitleMouseDown}
              onKeyDown={handleTitleKeyDown}
            >
              {note.title || "ToDo"}
            </span>
          )}

          {/* Close + Delete + New — right */}
          <div className="note-actions-right">
            <button
              className="btn-action btn-close-btn"
              onClick={handleClose}
              title="Hide"
            >
              ✕
            </button>
            <button
              className={`btn-action btn-delete-btn${!canDelete ? " btn-disabled" : ""}`}
              onClick={canDelete ? handleDelete : undefined}
              title={canDelete ? "Delete" : "Cannot delete the last note"}
              disabled={!canDelete}
            >
              🗑
            </button>
            <button
              className="btn-action btn-new-note-btn"
              onClick={handleNewNote}
              title="New Note"
            >
              +
            </button>
          </div>
        </header>

        {/* Todo list */}
        <div className="note-todo-list">
          {todos.map((todo) => (
            <div
              key={todo.id}
              className="todo-item"
              onMouseDown={(e) => e.stopPropagation()}
            >
              <input
                type="checkbox"
                checked={todo.status === "done"}
                onChange={() => handleToggleTodo(todo.id)}
              />
              <span className={`todo-text${todo.status === "done" ? " todo-done" : ""}`}>
                {todo.task}
              </span>
              <button
                className="btn-todo-delete"
                onClick={() => handleDeleteTodo(todo.id)}
                title="Delete"
              >
                ✕
              </button>
            </div>
          ))}

          {/* New todo input */}
          <div className="todo-item todo-new">
            <span className="todo-new-indicator">+</span>
            <input
              ref={newTodoRef}
              type="text"
              className="todo-new-input"
              value={newTodoText}
              onChange={(e) => setNewTodoText(e.target.value)}
              onKeyDown={(e) => {
                if (e.key === "Enter") {
                  e.preventDefault();
                  handleAddTodo();
                }
              }}
              placeholder="Add a todo..."
            />
          </div>
        </div>

        {/* Top fade gradient */}
        {showTopFade && <div className="note-fade-top" />}

        {/* Bottom fade gradient */}
        {showBottomFade && <div className="note-fade-bottom" />}
      </div>
    </div>
  );
}

const COLORS = [
  { name: "Yellow", hex: "#FFE066" },
  { name: "Green", hex: "#A8E6A1" },
  { name: "Blue", hex: "#87CEEB" },
  { name: "Pink", hex: "#FFB3C1" },
  { name: "Orange", hex: "#FFD4A1" },
  { name: "Purple", hex: "#D4B8E8" },
];
