import json
import uuid
from google import genai

# ----------------------------
# INIT CLIENT
# ----------------------------
client = genai.Client(api_key="YOUR API KEY")

# ----------------------------
# LLM CLASSIFIER
# ----------------------------
def classify_todo(text: str):
    prompt = f"""
You are a task-context prediction engine.

Your job is to decide:
1. What is the MOST LIKELY app or website where the user would complete this task?
2. You may return multiple contexts only if truly necessary.

Return ONLY valid JSON in this format:
{{
  "contexts": ["app_or_website_name"]
}}

Rules:
- Focus on the most likely execution environment, not general categories
- Be specific (e.g., "linkedin", "gmail", "github", "leetcode", "notion", "vscode", "youtube", "chatgpt", "google docs")
- If multiple are strongly relevant, include up to 3 max
- If uncertain, return ["general"]
- NEVER return "browser" as a context — always use the specific website or app name instead
- Do NOT explain anything
- Do NOT add markdown, text, or reasoning
- Output must be valid JSON

Examples:

Task: Ask Dave for a referral
Output:
{{ "contexts": ["linkedin"] }}

Task: Send follow-up email to recruiter
Output:
{{ "contexts": ["gmail"] }}

Task: Fix API bug in backend
Output:
{{ "contexts": ["vscode"] }}

Task: Practice binary tree problems
Output:
{{ "contexts": ["leetcode"] }}

Task: Write project documentation
Output:
{{ "contexts": ["notion", "google docs"] }}

Task: Watch the tutorial Dave posted
Output:
{{ "contexts": ["youtube"] }}

Task: Review my networking plan
Output:
{{ "contexts": ["chatgpt"] }}

Now classify this task:

Task:
{text}
"""

    response = client.models.generate_content(
        model="gemini-3.1-flash-lite",
        contents=prompt
    )

    try:
        return json.loads(response.text)
    except Exception:
        cleaned = response.text.strip().replace("```json", "").replace("```", "").strip()
        return json.loads(cleaned)

# ----------------------------
# TODO CREATOR
# ----------------------------
def create_todo(text: str):
    print("  🤖 Classifying context with AI...")
    classification = classify_todo(text)
    contexts = classification.get("contexts", ["general"])

    todo = {
        "id": str(uuid.uuid4()),
        "text": text,
        "contexts": contexts
    }

    return todo

# ----------------------------
# STORAGE (JSON FILE)
# ----------------------------
TODO_FILE = "todos.json"

def load_todos():
    try:
        with open(TODO_FILE, "r") as f:
            return json.load(f)
    except (FileNotFoundError, json.JSONDecodeError):
        return []

def save_todo(todo):
    data = load_todos()
    data.append(todo)
    with open(TODO_FILE, "w") as f:
        json.dump(data, f, indent=2)

def list_todos():
    data = load_todos()
    if not data:
        print("  (no todos yet)")
        return
    for i, t in enumerate(data, 1):
        ctx = ", ".join(t["contexts"])
        print(f"  {i}. [{ctx}] {t['text']}")

def delete_todo():
    data = load_todos()
    if not data:
        print("  (no todos to delete)")
        return
    list_todos()
    try:
        idx = int(input("\nEnter number to delete: ").strip()) - 1
        removed = data.pop(idx)
        with open(TODO_FILE, "w") as f:
            json.dump(data, f, indent=2)
        print(f"  ✅ Deleted: {removed['text']}")
    except (ValueError, IndexError):
        print("  ❌ Invalid selection.")

# ----------------------------
# MAIN LOOP
# ----------------------------
def main():
    print("=" * 50)
    print("  📝 Sticky Todo AI")
    print("=" * 50)
    print("  Commands:")
    print("    [Enter text]  → Add a new todo")
    print("    /list         → List all todos")
    print("    /delete       → Delete a todo")
    print("    /quit         → Exit")
    print("=" * 50)
    print()

    while True:
        try:
            text = input("➤ Todo: ").strip()
        except (KeyboardInterrupt, EOFError):
            print("\n\n👋 Exiting. Goodbye!")
            break

        if not text:
            continue

        if text.lower() in ("/quit", "/exit", "quit", "exit"):
            print("\n👋 Exiting. Goodbye!")
            break

        elif text.lower() == "/list":
            print()
            list_todos()
            print()

        elif text.lower() == "/delete":
            print()
            delete_todo()
            print()

        elif text.startswith("/"):
            print("  ❓ Unknown command. Try /list, /delete, or /quit.\n")

        else:
            todo = create_todo(text)
            save_todo(todo)
            ctx_display = ", ".join(todo["contexts"])
            print(f"\n  ✅ Saved! Context: [{ctx_display}]")
            print(f"     Task: {todo['text']}\n")

if __name__ == "__main__":
    main()
