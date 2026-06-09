import React from "react";
import ReactDOM from "react-dom/client";
import { getCurrentWindow } from "@tauri-apps/api/window";
import NoteWindow from "./components/NoteWindow";
import PreferencesWindow from "./components/PreferencesWindow";
import TodoPopupWindow from "./components/TodoPopupWindow";
import "./styles/global.css";

const currentWindow = getCurrentWindow();
const label = currentWindow.label;
const hash = window.location.hash;

let App: React.ComponentType;

if (label.startsWith("note-") || hash.startsWith("#note-")) {
  App = NoteWindow;
} else if (label === "preferences" || hash === "#preferences") {
  App = PreferencesWindow;
} else if (label.startsWith("todo-popup-") || hash.startsWith("#todo-popup")) {
  App = TodoPopupWindow;
} else {
  // Should never happen since no main window is configured
  App = () => <div style={{ padding: 20 }}>No UI loaded</div>;
}

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <App />
  </React.StrictMode>,
);
