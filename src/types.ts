export interface Note {
  id: string;
  title: string;
  x: number;
  y: number;
  width: number;
  height: number;
  color: string;
  todoIds: string[];
}

export interface TodoItem {
  id: string;
  task: string;
  status: "undone" | "done";
  seen: boolean;
}
