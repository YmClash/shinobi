"use client";

import { useState, useEffect } from "react";
import { useParams } from "next/navigation";

// ── Types ────────────────────────────────────────────────────────────────

interface ManifestFile {
  path: string;
  cid: string;
  size: number;
}

interface Manifest {
  version: number;
  description: string;
  files: ManifestFile[];
}

/** A single chat message parsed from Copilot JSONL or JSON. */
interface ChatMessage {
  role: "user" | "assistant" | "system" | string;
  content: string;
  timestamp?: string;
}

// ── Page Component ───────────────────────────────────────────────────────

export default function IpfsViewerPage() {
  const params = useParams<{ cid: string }>();
  const cid = params.cid;

  const [manifest, setManifest] = useState<Manifest | null>(null);
  const [selectedFile, setSelectedFile] = useState<ManifestFile | null>(null);
  const [fileContent, setFileContent] = useState<string>("");
  const [loading, setLoading] = useState(true);
  const [fileLoading, setFileLoading] = useState(false);
  const [error, setError] = useState<string>("");

  // Fetch the root manifest
  useEffect(() => {
    if (!cid) return;
    setLoading(true);
    fetch(`/api/ipfs/${cid}`)
      .then((r) => {
        if (!r.ok) throw new Error(`IPFS gateway returned ${r.status}`);
        return r.json();
      })
      .then((data: Manifest) => {
        setManifest(data);
        setLoading(false);
      })
      .catch((e) => {
        setError(e.message);
        setLoading(false);
      });
  }, [cid]);

  // Fetch individual file content
  function openFile(file: ManifestFile) {
    setSelectedFile(file);
    setFileContent("");

    // Images don't need text fetch — render directly via <img src>
    if (isImageFile(file.path)) {
      setFileLoading(false);
      return;
    }

    setFileLoading(true);
    fetch(`/api/ipfs/${file.cid}`)
      .then((r) => r.text())
      .then((text) => {
        setFileContent(text);
        setFileLoading(false);
      })
      .catch(() => {
        setFileContent("Failed to load file from IPFS.");
        setFileLoading(false);
      });
  }

  if (loading) {
    return (
      <div className="ipfs-viewer-loading">
        <div className="ipfs-viewer-spinner" />
        <p>Loading IPFS content...</p>
      </div>
    );
  }

  if (error) {
    return (
      <div className="ipfs-viewer-error">
        <span>⚠️</span>
        <p>{error}</p>
      </div>
    );
  }

  return (
    <div className="ipfs-viewer-page">
      {/* Header */}
      <div className="ipfs-viewer-header">
        <div className="ipfs-viewer-header-left">
          <span className="ipfs-viewer-header-icon">📦</span>
          <div>
            <h1 className="ipfs-viewer-title">IPFS Artifact Viewer</h1>
            <p className="ipfs-viewer-subtitle">
              {manifest?.description || "Checkpoint artifacts"}
            </p>
          </div>
        </div>
        <code className="ipfs-viewer-cid">{cid}</code>
      </div>

      <div className="ipfs-viewer-layout">
        {/* Sidebar — File list */}
        <aside className="ipfs-viewer-sidebar">
          <div className="ipfs-viewer-sidebar-header">
            Files ({manifest?.files.length ?? 0})
          </div>
          {manifest?.files.map((f) => (
            <button
              key={f.cid}
              className={`ipfs-viewer-file-btn ${selectedFile?.cid === f.cid ? "active" : ""}`}
              onClick={() => openFile(f)}
            >
              <span className="ipfs-viewer-file-icon">{fileIcon(f.path)}</span>
              <div className="ipfs-viewer-file-info">
                <span className="ipfs-viewer-file-name">{f.path}</span>
                <span className="ipfs-viewer-file-size">
                  {formatSize(f.size)}
                </span>
              </div>
            </button>
          ))}
        </aside>

        {/* Content area */}
        <main className="ipfs-viewer-content">
          {!selectedFile ? (
            <div className="ipfs-viewer-placeholder">
              <span className="ipfs-viewer-placeholder-icon">👈</span>
              <p>Select a file to view its contents</p>
            </div>
          ) : fileLoading ? (
            <div className="ipfs-viewer-loading">
              <div className="ipfs-viewer-spinner" />
              <p>Loading {selectedFile.path}...</p>
            </div>
          ) : (
            <FileRenderer file={selectedFile} content={fileContent} />
          )}
        </main>
      </div>
    </div>
  );
}

// ── File Renderer ────────────────────────────────────────────────────────

function FileRenderer({
  file,
  content,
}: {
  file: ManifestFile;
  content: string;
}) {
  const ext = file.path.split(".").pop()?.toLowerCase() || "";

  // Images → render via <img> tag (no text fetch needed)
  if (isImageFile(file.path)) {
    return <ImageViewer file={file} />;
  }

  // JSONL / JSON / TXT containing chat data → bubble view
  if (
    ext === "jsonl" ||
    (ext === "json" && looksLikeChat(content)) ||
    (ext === "txt" && looksLikeChat(content))
  ) {
    const messages = parseChatMessages(content);
    if (messages.length > 0) {
      return <ChatBubbleViewer messages={messages} filename={file.path} />;
    }
  }

  // Markdown → formatted view
  if (ext === "md") {
    return <MarkdownViewer content={content} filename={file.path} />;
  }

  // Default → monospace
  return <PlainTextViewer content={content} filename={file.path} />;
}

// ── Chat Bubble Viewer (Copilot / Antigravity) ──────────────────────────

function ChatBubbleViewer({
  messages,
  filename,
}: {
  messages: ChatMessage[];
  filename: string;
}) {
  return (
    <div className="chat-viewer">
      <div className="chat-viewer-header">
        <span className="chat-viewer-header-icon">💬</span>
        <span className="chat-viewer-header-title">{filename}</span>
        <span className="chat-viewer-header-count">
          {messages.length} messages
        </span>
      </div>
      <div className="chat-viewer-messages">
        {messages.map((msg, i) => {
          const roleClass =
            msg.role === "user" ? "user"
            : msg.role === "tool" ? "tool"
            : msg.role === "system" ? "system"
            : "assistant";
          const avatar =
            msg.role === "user" ? "👤"
            : msg.role === "tool" ? "🔧"
            : msg.role === "system" ? "⚙️"
            : "🤖";
          const label =
            msg.role === "user" ? "You"
            : msg.role === "tool" ? "Tool"
            : msg.role === "system" ? "System"
            : "AI Assistant";

          return (
            <div key={i} className={`chat-bubble chat-bubble-${roleClass}`}>
              <div className="chat-bubble-avatar">{avatar}</div>
              <div className="chat-bubble-body">
                <div className="chat-bubble-role">
                  {label}
                  {msg.timestamp && (
                    <span className="chat-bubble-time">
                      {formatTimestamp(msg.timestamp)}
                    </span>
                  )}
                </div>
                <div className="chat-bubble-content">
                  {renderContent(msg.content)}
                </div>
              </div>
            </div>
          );
        })}
      </div>
    </div>
  );
}

// ── Markdown Viewer ──────────────────────────────────────────────────────

function MarkdownViewer({
  content,
  filename,
}: {
  content: string;
  filename: string;
}) {
  // Simple markdown rendering — headings, bold, code blocks, lists
  const html = simpleMarkdownToHtml(content);

  return (
    <div className="md-viewer">
      <div className="md-viewer-header">
        <span className="md-viewer-header-icon">📝</span>
        <span>{filename}</span>
      </div>
      <div
        className="md-viewer-body"
        dangerouslySetInnerHTML={{ __html: html }}
      />
    </div>
  );
}

// ── Plain Text Viewer ────────────────────────────────────────────────────

function PlainTextViewer({
  content,
  filename,
}: {
  content: string;
  filename: string;
}) {
  const lines = content.split("\n");
  return (
    <div className="text-viewer">
      <div className="text-viewer-header">
        <span className="text-viewer-header-icon">📄</span>
        <span>{filename}</span>
        <span className="text-viewer-line-count">{lines.length} lines</span>
      </div>
      <pre className="text-viewer-body">
        <code>
          {lines.map((line, i) => (
            <div key={i} className="text-viewer-line">
              <span className="text-viewer-lineno">{i + 1}</span>
              <span className="text-viewer-text">{line}</span>
            </div>
          ))}
        </code>
      </pre>
    </div>
  );
}

// ── Parsers & Helpers ────────────────────────────────────────────────────

/** Parse chat messages from JSONL or JSON */
function parseChatMessages(raw: string): ChatMessage[] {
  const messages: ChatMessage[] = [];

  // Try JSON first (array of messages or { messages: [...] })
  try {
    const json = JSON.parse(raw);
    if (Array.isArray(json)) {
      for (const msg of json) {
        const parsed = extractMessage(msg);
        if (parsed) messages.push(parsed);
      }
      if (messages.length > 0) return messages;
    }
    if (json.messages && Array.isArray(json.messages)) {
      for (const msg of json.messages) {
        const parsed = extractMessage(msg);
        if (parsed) messages.push(parsed);
      }
      if (messages.length > 0) return messages;
    }
  } catch {
    // Not valid JSON — try JSONL
  }

  // JSONL: line by line
  for (const line of raw.split("\n")) {
    const trimmed = line.trim();
    if (!trimmed) continue;
    try {
      const obj = JSON.parse(trimmed);
      const parsed = extractMessage(obj);
      if (parsed) messages.push(parsed);
    } catch {
      // skip invalid lines
    }
  }

  return messages;
}

/** Extract a ChatMessage from various AI agent JSON formats */
function extractMessage(obj: Record<string, unknown>): ChatMessage | null {
  // ── Format 1: Standard { role, content } ──────────────────
  if (obj.role && obj.content) {
    return {
      role: obj.role as string,
      content: obj.content as string,
      timestamp: (obj.timestamp as string) || (obj.created_at as string) || undefined,
    };
  }

  // ── Format 2: Copilot { type: "user.message", text: "..." } ──
  if (obj.type && obj.text) {
    const type = obj.type as string;
    let role = "system";
    if (type.includes("user")) role = "user";
    else if (type.includes("assistant")) role = "assistant";
    return {
      role,
      content: obj.text as string,
      timestamp: (obj.timestamp as string) || undefined,
    };
  }

  // ── Format 3: Antigravity overview.txt ─────────────────────
  // { source: "USER_EXPLICIT"|"MODEL", type: "USER_INPUT"|"PLANNER_RESPONSE", content: "..." }
  if (obj.source && obj.content && typeof obj.content === "string") {
    const source = obj.source as string;
    let role = "system";
    if (source === "USER_EXPLICIT" || source === "USER") role = "user";
    else if (source === "MODEL") role = "assistant";
    return {
      role,
      content: obj.content as string,
      timestamp: (obj.created_at as string) || undefined,
    };
  }

  // ── Format 4: Copilot enriched { type: "...", data: { content|text } } ──
  if (obj.type && obj.data && typeof obj.data === "object") {
    const type = obj.type as string;
    const data = obj.data as Record<string, unknown>;
    const dataContent = (data.content || data.text || data.messageText || "") as string;

    // user.message / user_message / assistant.message / assistant_message
    const isUser = type === "user.message" || type === "user_message";
    const isAssistant = type === "assistant.message" || type === "assistant_message";

    if ((isUser || isAssistant) && dataContent) {
      return {
        role: isUser ? "user" : "assistant",
        content: dataContent,
        timestamp: (obj.timestamp as string) || undefined,
      };
    }

    // assistant.turn_start / assistant.turn_end — skip silently
    if (type.includes("turn_start") || type.includes("turn_end")) {
      return null;
    }

    // tool_execution_start / tool.execution_start
    if (type.includes("tool_execution_start") || type.includes("tool.execution_start")) {
      const toolName = (data.toolName || data.name || "unknown") as string;
      const args = data.arguments ? String(data.arguments).slice(0, 200) : "";
      return {
        role: "tool",
        content: `🔧 **${toolName}**${args ? `\n\`${args}\`` : ""}`,
        timestamp: (obj.timestamp as string) || undefined,
      };
    }

    if (type.includes("tool_execution_complete") || type.includes("tool.execution_complete")) {
      const success = data.success === true ? "✅" : "❌";
      const output = (data.output as string) || "";
      return {
        role: "tool",
        content: `${success} Tool completed${output ? `\n${output.slice(0, 300)}` : ""}`,
        timestamp: (obj.timestamp as string) || undefined,
      };
    }

    // session.start / session_start — display as system info
    if (type === "session.start" || type === "session_start") {
      const sessionId = (data.sessionId || data.session_id || "") as string;
      return {
        role: "system",
        content: `📋 Session started${sessionId ? ` · ${sessionId}` : ""}`,
        timestamp: (obj.timestamp as string) || undefined,
      };
    }

    // Catch-all: if data has content and type looks message-like
    if (dataContent && (type.includes("message") || type.includes("response"))) {
      let role = "system";
      if (type.includes("user")) role = "user";
      else if (type.includes("assistant")) role = "assistant";
      return {
        role,
        content: dataContent,
        timestamp: (obj.timestamp as string) || undefined,
      };
    }
  }

  return null;
}

/** Check if a JSON string looks like a chat conversation */
function looksLikeChat(content: string): boolean {
  // Quick heuristic: first non-empty line starts with {
  const firstLine = content.trimStart().split("\n")[0]?.trim() || "";
  if (!firstLine.startsWith("{")) return false;

  return (
    content.includes('"role"') ||
    content.includes('"user.message"') ||
    content.includes('"assistant.message"') ||
    content.includes('"USER_EXPLICIT"') ||
    content.includes('"USER_INPUT"') ||
    content.includes('"PLANNER_RESPONSE"') ||
    content.includes('"user_message"') ||
    content.includes('"assistant_message"') ||
    content.includes('"session_start"') ||
    content.includes('"session.start"')
  );
}

/** Render message content, handling code blocks */
function renderContent(text: string) {
  // Split on code blocks ```...```
  const parts = text.split(/(```[\s\S]*?```)/g);
  return parts.map((part, i) => {
    if (part.startsWith("```")) {
      const lines = part.split("\n");
      const lang = lines[0].replace("```", "").trim();
      const code = lines.slice(1, -1).join("\n");
      return (
        <pre key={i} className="chat-code-block">
          {lang && <span className="chat-code-lang">{lang}</span>}
          <code>{code}</code>
        </pre>
      );
    }
    // Inline code
    const inlined = part.split(/(`[^`]+`)/g);
    return (
      <span key={i}>
        {inlined.map((seg, j) =>
          seg.startsWith("`") ? (
            <code key={j} className="chat-inline-code">
              {seg.slice(1, -1)}
            </code>
          ) : (
            <span key={j}>{seg}</span>
          ),
        )}
      </span>
    );
  });
}

/** Simple markdown → HTML (headings, bold, italic, lists, code blocks) */
function simpleMarkdownToHtml(md: string): string {
  let html = md
    // Escape HTML entities
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;")
    // Code blocks
    .replace(/```(\w*)\n([\s\S]*?)```/g, '<pre class="md-code-block"><code>$2</code></pre>')
    // Headings
    .replace(/^#### (.+)$/gm, '<h4 class="md-h">$1</h4>')
    .replace(/^### (.+)$/gm, '<h3 class="md-h">$1</h3>')
    .replace(/^## (.+)$/gm, '<h2 class="md-h">$1</h2>')
    .replace(/^# (.+)$/gm, '<h1 class="md-h">$1</h1>')
    // Bold & Italic
    .replace(/\*\*(.+?)\*\*/g, "<strong>$1</strong>")
    .replace(/\*(.+?)\*/g, "<em>$1</em>")
    // Inline code
    .replace(/`([^`]+)`/g, '<code class="md-inline-code">$1</code>')
    // Unordered lists
    .replace(/^- (.+)$/gm, '<li class="md-li">$1</li>')
    // Horizontal rules
    .replace(/^---$/gm, '<hr class="md-hr" />')
    // Line breaks → paragraphs
    .replace(/\n\n/g, '</p><p class="md-p">')
    .replace(/\n/g, "<br />");

  return `<p class="md-p">${html}</p>`;
}

// ── Image Viewer ─────────────────────────────────────────────────────────

function ImageViewer({ file }: { file: ManifestFile }) {
  return (
    <div className="image-viewer">
      <div className="image-viewer-header">
        <span className="image-viewer-header-icon">🖼️</span>
        <span>{file.path}</span>
        <span className="image-viewer-size">{formatSize(file.size)}</span>
      </div>
      <div className="image-viewer-body">
        {/* eslint-disable-next-line @next/next/no-img-element */}
        <img
          src={`/api/ipfs/${file.cid}`}
          alt={file.path}
          className="image-viewer-img"
          loading="lazy"
        />
      </div>
    </div>
  );
}

const IMAGE_EXTENSIONS = new Set(["png", "jpg", "jpeg", "gif", "webp", "svg", "ico", "bmp"]);

function isImageFile(path: string): boolean {
  const ext = path.split(".").pop()?.toLowerCase() || "";
  return IMAGE_EXTENSIONS.has(ext);
}

function fileIcon(path: string): string {
  const ext = path.split(".").pop()?.toLowerCase() || "";
  if (IMAGE_EXTENSIONS.has(ext)) return "🖼️";
  switch (ext) {
    case "md": return "📝";
    case "json": case "jsonl": return "💬";
    case "txt": return "📄";
    default: return "📎";
  }
}

function formatSize(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
}

function formatTimestamp(ts: string): string {
  try {
    return new Date(ts).toLocaleTimeString([], {
      hour: "2-digit",
      minute: "2-digit",
    });
  } catch {
    return ts;
  }
}
