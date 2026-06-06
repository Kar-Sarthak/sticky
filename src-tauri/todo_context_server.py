"""
Sticky Notes Context Classifier Server
---------------------------------------
Lightweight HTTP server that classifies todo text into app/website contexts
using Google's Gemini API. Runs alongside the Tauri app.

Endpoints:
  POST /classify  - {"text": "task description"} → {"contexts": ["app1", "app2"]}
  GET  /health    - {"status": "ok"}
"""

import json
import sys
from http.server import HTTPServer, BaseHTTPRequestHandler

try:
    from google import genai
except ImportError:
    print("ERROR: google-genai not installed. Run: pip install google-genai", file=sys.stderr)
    sys.exit(1)

# ─── CONFIG ────────────────────────────────────────────────────────────
HOST = "127.0.0.1"
PORT = 8765
API_KEY = ""  # Replace with your actual API key
MODEL = "gemini-3.1-flash-lite"

client = genai.Client(api_key=API_KEY)

# ─── CLASSIFIER ────────────────────────────────────────────────────────
CLASSIFY_PROMPT = """
You are a task-context prediction engine.

Return ONLY valid JSON in this format:
{{ "contexts": ["app_or_website_name"] }}

Rules:
- Focus on the most likely execution environment
- Be specific (e.g., "linkedin", "gmail", "github", "leetcode", "notion", "vscode", "youtube", "chatgpt", "google docs", "figma", "slack", "discord", "calendar")
- Include up to 3 contexts max if multiple are strongly relevant
- If uncertain, return ["general"]
- Do NOT add markdown, text, or reasoning
- Output must be valid JSON

Task: {text}
"""


def classify_todo(text: str) -> list[str]:
    prompt = CLASSIFY_PROMPT.format(text=text)
    response = client.models.generate_content(model=MODEL, contents=prompt)
    try:
        data = json.loads(response.text)
        return data.get("contexts", ["general"])
    except Exception:
        cleaned = response.text.strip().replace("```json", "").replace("```", "").strip()
        try:
            data = json.loads(cleaned)
            return data.get("contexts", ["general"])
        except Exception:
            return ["general"]


# ─── HTTP SERVER ───────────────────────────────────────────────────────
class Handler(BaseHTTPRequestHandler):
    def do_GET(self):
        if self.path == "/health":
            self._json(200, {"status": "ok"})
        else:
            self._json(404, {"error": "not found"})

    def do_POST(self):
        if self.path != "/classify":
            self._json(404, {"error": "not found"})
            return

        content_length = int(self.headers.get("Content-Length", 0))
        body = self.rfile.read(content_length)
        try:
            data = json.loads(body)
            text = data.get("text", "")
        except (json.JSONDecodeError, KeyError):
            self._json(400, {"error": "invalid json, expected {\"text\": \"...\"}"})
            return

        if not text.strip():
            self._json(400, {"error": "text is required"})
            return

        try:
            contexts = classify_todo(text)
            self._json(200, {"contexts": contexts})
        except Exception as e:
            self._json(500, {"error": str(e), "contexts": ["general"]})

    def _json(self, status: int, data: dict):
        body = json.dumps(data).encode("utf-8")
        self.send_response(status)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)

    def log_message(self, format, *args):
        # Suppress default stderr logging
        pass


# ─── MAIN ──────────────────────────────────────────────────────────────
def main():
    server = HTTPServer((HOST, PORT), Handler)
    print(f"Context classifier running on http://{HOST}:{PORT}", flush=True)
    try:
        server.serve_forever()
    except KeyboardInterrupt:
        print("\nShutting down context classifier...", flush=True)
        server.shutdown()


if __name__ == "__main__":
    main()
