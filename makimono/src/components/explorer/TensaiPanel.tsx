"use client";

// ═══════════════════════════════════════════════════════════════
// TensaiPanel — Panneau IA contextuel (Phase 14)
// Slide-over droit avec recherche sémantique RAG.
// Le LLM a le contexte du fichier ouvert.
// ═══════════════════════════════════════════════════════════════

import React, { useState, useRef, useEffect } from "react";
import { createPortal } from "react-dom";
import {
  X,
  Sparkles,
  Send,
  FileCode,
  Cpu,
  Loader2,
  Search,
  Zap,
  BookOpen,
} from "lucide-react";
import { semanticSearch, type SemanticChunk } from "@/lib/api";

interface TensaiPanelProps {
  isOpen: boolean;
  onClose: () => void;
  filePath: string;
  fileContent: string;
  language: string | null;
}

interface ChatMessage {
  id: string;
  role: "user" | "assistant" | "system";
  content: string;
  chunks?: SemanticChunk[];
  timestamp: Date;
}

export default function TensaiPanel({
  isOpen,
  onClose,
  filePath,
  fileContent,
  language,
}: TensaiPanelProps) {
  const [query, setQuery] = useState("");
  const [messages, setMessages] = useState<ChatMessage[]>([]);
  const [isLoading, setIsLoading] = useState(false);
  const inputRef = useRef<HTMLInputElement>(null);
  const messagesEndRef = useRef<HTMLDivElement>(null);
  const filename = filePath.split("/").pop() ?? filePath;
  const lineCount = fileContent.split("\n").length;

  // Auto-focus input when panel opens
  useEffect(() => {
    if (isOpen) {
      setTimeout(() => inputRef.current?.focus(), 200);
    }
  }, [isOpen]);

  // Scroll to bottom on new message
  useEffect(() => {
    messagesEndRef.current?.scrollIntoView({ behavior: "smooth" });
  }, [messages]);

  // ── Quick Actions ──────────────────────────────────────────
  const quickActions = [
    {
      label: "Expliquer ce fichier",
      icon: BookOpen,
      query: `Explique le fichier ${filename} et son rôle dans le projet`,
    },
    {
      label: "Trouver des bugs",
      icon: Search,
      query: `Analyse le fichier ${filename} pour trouver des bugs potentiels ou des problèmes de sécurité`,
    },
    {
      label: "Optimiser",
      icon: Zap,
      query: `Comment optimiser les performances du code dans ${filename}`,
    },
  ];

  // ── Send Query ─────────────────────────────────────────────
  const handleSend = async (inputQuery?: string) => {
    const q = inputQuery ?? query;
    if (!q.trim() || isLoading) return;

    const contextualQuery = `${q} (contexte: fichier ${filePath}, langage ${language ?? "inconnu"})`;

    // Add user message
    const userMsg: ChatMessage = {
      id: crypto.randomUUID(),
      role: "user",
      content: q,
      timestamp: new Date(),
    };
    setMessages((prev) => [...prev, userMsg]);
    setQuery("");
    setIsLoading(true);

    try {
      const result = await semanticSearch(contextualQuery, 5, 0.3);

      // Build assistant response
      const chunkSummary =
        result.chunks.length > 0
          ? result.chunks
              .map(
                (c, i) =>
                  `**${i + 1}. ${c.name ?? c.file_path}** (${c.language}, L${c.start_line}–${c.end_line}, similarité: ${Math.round(c.similarity * 100)}%)\n\`\`\`${c.language}\n${c.content.slice(0, 300)}${c.content.length > 300 ? "\n// ..." : ""}\n\`\`\``
              )
              .join("\n\n")
          : "Aucun fragment de code similaire trouvé dans l'index sémantique.";

      const responseContent =
        result.chunks.length > 0
          ? `🧠 **${result.count} fragment${result.count > 1 ? "s" : ""} trouvé${result.count > 1 ? "s" : ""}** en rapport avec votre question :\n\n${chunkSummary}`
          : `🔍 Aucun résultat pertinent pour "${q}". Essayez de reformuler ou vérifiez que le code a été indexé (Tensai doit avoir analysé au moins une opération sur ce dépôt).`;

      const assistantMsg: ChatMessage = {
        id: crypto.randomUUID(),
        role: "assistant",
        content: responseContent,
        chunks: result.chunks,
        timestamp: new Date(),
      };
      setMessages((prev) => [...prev, assistantMsg]);
    } catch (err) {
      const errorMsg: ChatMessage = {
        id: crypto.randomUUID(),
        role: "system",
        content: `⚠️ Erreur de recherche : ${err instanceof Error ? err.message : "Erreur inconnue"}. Vérifiez que Taijutsu est en ligne.`,
        timestamp: new Date(),
      };
      setMessages((prev) => [...prev, errorMsg]);
    } finally {
      setIsLoading(false);
    }
  };

  const handleKeyDown = (e: React.KeyboardEvent) => {
    if (e.key === "Enter" && !e.shiftKey) {
      e.preventDefault();
      handleSend();
    }
  };

  if (!isOpen) return null;

  return createPortal(
    <div className="tensai-overlay" onClick={onClose}>
      <div className="tensai-panel" onClick={(e) => e.stopPropagation()}>
        {/* ── Header ──────────────────────────────────── */}
        <div className="tensai-header">
          <div className="tensai-header-left">
            <Sparkles size={18} className="tensai-icon-sparkle" />
            <span className="tensai-title">Tensai IA</span>
          </div>
          <button className="tensai-close" onClick={onClose} title="Fermer">
            <X size={18} />
          </button>
        </div>

        {/* ── File Context ────────────────────────────── */}
        <div className="tensai-context">
          <FileCode size={14} />
          <span className="tensai-context-file">{filename}</span>
          <span className="tensai-context-meta">
            {language ?? "text"} · {lineCount} lignes
          </span>
        </div>

        {/* ── Messages ────────────────────────────────── */}
        <div className="tensai-messages">
          {messages.length === 0 && (
            <div className="tensai-welcome">
              <div className="tensai-welcome-icon">
                <Cpu size={32} />
              </div>
              <h3>Interrogez votre code</h3>
              <p>
                Posez une question sur <strong>{filename}</strong> ou explorez
                la base de connaissances sémantique de votre projet.
              </p>

              {/* Quick Actions */}
              <div className="tensai-quick-actions">
                {quickActions.map((action) => (
                  <button
                    key={action.label}
                    className="tensai-quick-btn"
                    onClick={() => handleSend(action.query)}
                  >
                    <action.icon size={14} />
                    <span>{action.label}</span>
                  </button>
                ))}
              </div>
            </div>
          )}

          {messages.map((msg) => (
            <div
              key={msg.id}
              className={`tensai-msg tensai-msg-${msg.role}`}
            >
              <div className="tensai-msg-content">
                {msg.role === "user" ? (
                  <p>{msg.content}</p>
                ) : msg.role === "system" ? (
                  <p className="tensai-msg-error">{msg.content}</p>
                ) : (
                  <div
                    className="tensai-msg-md"
                    dangerouslySetInnerHTML={{
                      __html: simpleMarkdown(msg.content),
                    }}
                  />
                )}
              </div>
              <span className="tensai-msg-time">
                {msg.timestamp.toLocaleTimeString("fr-FR", {
                  hour: "2-digit",
                  minute: "2-digit",
                })}
              </span>
            </div>
          ))}

          {isLoading && (
            <div className="tensai-msg tensai-msg-assistant">
              <div className="tensai-msg-loading">
                <Loader2 size={16} className="tensai-spinner" />
                <span>Recherche sémantique en cours…</span>
              </div>
            </div>
          )}

          <div ref={messagesEndRef} />
        </div>

        {/* ── Input ───────────────────────────────────── */}
        <div className="tensai-input-bar">
          <input
            ref={inputRef}
            type="text"
            className="tensai-input"
            placeholder={`Poser une question sur ${filename}…`}
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            onKeyDown={handleKeyDown}
            disabled={isLoading}
          />
          <button
            className="tensai-send"
            onClick={() => handleSend()}
            disabled={!query.trim() || isLoading}
            title="Envoyer"
          >
            <Send size={16} />
          </button>
        </div>
      </div>
    </div>,
    document.body,
  );
}

// ── Simple Markdown → HTML (inline, no deps) ─────────────────
function simpleMarkdown(text: string): string {
  return text
    .replace(/```(\w+)?\n([\s\S]*?)```/g, '<pre class="tensai-code-block"><code>$2</code></pre>')
    .replace(/`([^`]+)`/g, '<code class="tensai-inline-code">$1</code>')
    .replace(/\*\*([^*]+)\*\*/g, "<strong>$1</strong>")
    .replace(/\*([^*]+)\*/g, "<em>$1</em>")
    .replace(/\n/g, "<br/>");
}
