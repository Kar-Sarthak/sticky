use serde::{Deserialize, Serialize};

/// A single todo item stored globally in todos.json.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TodoItem {
    pub id: String,
    pub task: String,
    pub status: String, // "undone" | "done"
}

/// A sticky note. Instead of raw content, stores an ordered list of todo IDs.
/// The actual todo data lives in todos.json.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Note {
    pub id: String,
    pub title: String,
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    pub color: String,         // hex color, e.g. "#FFE066"
    pub todo_ids: Vec<String>, // ordered list of todo IDs belonging to this note
}
