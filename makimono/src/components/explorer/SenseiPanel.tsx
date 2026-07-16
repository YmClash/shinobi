"use client";

// ═══════════════════════════════════════════════════════════════
// SenseiPanel — Agent Conversationnel IA (Phase 15 — 先生)
//
// Chat streaming en temps réel avec l'agent Sensei.
// Orchestre : Tensai (RAG) + Oracle (Reviews) + LLM (Ollama #2).
//
// Features :
//   - SSE streaming token par token (effet ChatGPT)
//   - Sources RAG affichées comme badges cliquables
//   - Badge Oracle score coloré
//   - Historique multi-tour envoyé au backend
//   - Bouton Stop pour annuler la génération
//   - Quick Actions (Expliquer, Bugs, Optimiser)
// ═══════════════════════════════════════════════════════════════

import React, { useState, useRef, useEffect, useCallback } from "react";
import { createPortal } from "react-dom";
import {
  X,
  Sparkles,
  Send,
  FileCode,
  Loader2,
  Search,
  Zap,
  BookOpen,
  StopCircle,
  Clock,
  Shield,
} from "lucide-react";
import {
  senseiChatStream,
  getSenseiModels,
  warmupSenseiModel,
  type SenseiMessage,
  type SenseiSource,
  type SenseiModelInfo,
} from "@/lib/api";

interface SenseiPanelProps {
  isOpen: boolean;
  onClose: () => void;
  filePath: string;
  fileContent: string;
  language: string | null;
  owner: string;
  repo: string;
}

interface ChatMessage {
  id: string;
  role: "user" | "assistant" | "system";
  content: string;
  sources?: SenseiSource[];
  oracleScore?: number | null;
  oracleSummary?: string | null;
  model?: string;
  durationMs?: number;
  timestamp: Date;
  isStreaming?: boolean;
}

export default function SenseiPanel({
  isOpen,
  onClose,
  filePath,
  fileContent,
  language,
  owner,
  repo,
}: SenseiPanelProps) {
  const [query, setQuery] = useState("");
  const [messages, setMessages] = useState<ChatMessage[]>([]);
  const [isStreaming, setIsStreaming] = useState(false);
  const [activeModel, setActiveModel] = useState<string | null>(null);
  const [availableModels, setAvailableModels] = useState<SenseiModelInfo[]>([]);
  const [isWarmingUp, setIsWarmingUp] = useState(false);
  const inputRef = useRef<HTMLInputElement>(null);
  const messagesEndRef = useRef<HTMLDivElement>(null);
  const abortControllerRef = useRef<AbortController | null>(null);
  const filename = filePath.split("/").pop() ?? filePath;
  const lineCount = fileContent.split("\n").length;

  // Auto-focus input + fetch models when panel opens
  useEffect(() => {
    if (isOpen) {
      setTimeout(() => inputRef.current?.focus(), 200);
      // Charger la liste des modèles et déclencher le warmup du modèle actif
      getSenseiModels()
        .then((data) => {
          setAvailableModels(data.models);
          if (!activeModel && data.active) {
            setActiveModel(data.active);
          }
          // Auto-warmup du modèle actif à l'ouverture
          const modelToWarm = activeModel || data.active;
          if (modelToWarm) {
            setIsWarmingUp(true);
            warmupSenseiModel(modelToWarm)
              .catch(() => {}) // silencieux
              .finally(() => setIsWarmingUp(false));
          }
        })
        .catch(() => {}); // silencieux si Sensei est désactivé
    }
  }, [isOpen]); // eslint-disable-line react-hooks/exhaustive-deps

  // Handler pour changer de modèle
  const handleModelChange = useCallback(async (modelName: string) => {
    setActiveModel(modelName);
    setIsWarmingUp(true);
    try {
      await warmupSenseiModel(modelName);
    } catch {
      // silencieux
    } finally {
      setIsWarmingUp(false);
    }
  }, []);

  // Scroll to bottom on new message
  useEffect(() => {
    messagesEndRef.current?.scrollIntoView({ behavior: "smooth" });
  }, [messages]);

  // ── Quick Actions ──────────────────────────────────────────
  const quickActions = [
    {
      label: "Expliquer",
      icon: BookOpen,
      query: `Explique le fichier ${filename} et son rôle dans le projet`,
    },
    {
      label: "Bugs",
      icon: Search,
      query: `Analyse ${filename} pour trouver des bugs potentiels ou des problèmes de sécurité`,
    },
    {
      label: "Optimiser",
      icon: Zap,
      query: `Comment optimiser les performances du code dans ${filename}`,
    },
  ];

  // ── Stop Generation ───────────────────────────────────────
  const handleStop = useCallback(() => {
    if (abortControllerRef.current) {
      abortControllerRef.current.abort();
      abortControllerRef.current = null;
      setIsStreaming(false);

      // Mark last assistant message as no longer streaming
      setMessages((prev) =>
        prev.map((msg, i) =>
          i === prev.length - 1 && msg.role === "assistant"
            ? { ...msg, isStreaming: false }
            : msg,
        ),
      );
    }
  }, []);

  // ── Send Query ─────────────────────────────────────────────
  const handleSend = useCallback(
    (inputQuery?: string) => {
      const q = inputQuery ?? query;
      if (!q.trim() || isStreaming) return;

      // Build conversation history for multi-turn
      const history: SenseiMessage[] = messages
        .filter((m) => m.role === "user" || m.role === "assistant")
        .map((m) => ({
          role: m.role as "user" | "assistant",
          content: m.content,
        }));

      // Add user message
      const userMsg: ChatMessage = {
        id: crypto.randomUUID(),
        role: "user",
        content: q,
        timestamp: new Date(),
      };

      // Add placeholder assistant message for streaming
      const assistantMsgId = crypto.randomUUID();
      const assistantMsg: ChatMessage = {
        id: assistantMsgId,
        role: "assistant",
        content: "",
        timestamp: new Date(),
        isStreaming: true,
      };

      setMessages((prev) => [...prev, userMsg, assistantMsg]);
      setQuery("");
      setIsStreaming(true);

      // Truncate file content for the prompt (~3000 chars)
      const truncatedContent =
        fileContent.length > 3000
          ? fileContent.slice(0, 3000) + "\n// ... (tronqué)"
          : fileContent;

      // Start SSE stream
      const controller = senseiChatStream(
        {
          query: q,
          file_path: filePath,
          file_content: truncatedContent || undefined,
          language,
          owner,
          repo,
          history,
        },
        {
          onContext: (sources, oracleScore, oracleSummary) => {
            // Update the assistant message with context metadata
            setMessages((prev) =>
              prev.map((msg) =>
                msg.id === assistantMsgId
                  ? { ...msg, sources, oracleScore, oracleSummary }
                  : msg,
              ),
            );
          },
          onToken: (token) => {
            // Append token to the streaming message
            setMessages((prev) =>
              prev.map((msg) =>
                msg.id === assistantMsgId
                  ? { ...msg, content: msg.content + token }
                  : msg,
              ),
            );
          },
          onDone: (model, durationMs) => {
            setIsStreaming(false);
            setActiveModel(model);
            abortControllerRef.current = null;
            // Finalize the assistant message
            setMessages((prev) =>
              prev.map((msg) =>
                msg.id === assistantMsgId
                  ? { ...msg, isStreaming: false, model, durationMs }
                  : msg,
              ),
            );
          },
          onError: (error) => {
            setIsStreaming(false);
            abortControllerRef.current = null;
            setMessages((prev) =>
              prev.map((msg) =>
                msg.id === assistantMsgId
                  ? {
                      ...msg,
                      role: "system" as const,
                      content: `⚠️ Erreur Sensei : ${error}`,
                      isStreaming: false,
                    }
                  : msg,
              ),
            );
          },
        },
      );

      abortControllerRef.current = controller;
    },
    [
      query,
      isStreaming,
      messages,
      filePath,
      fileContent,
      language,
      owner,
      repo,
    ],
  );

  const handleKeyDown = (e: React.KeyboardEvent) => {
    if (e.key === "Enter" && !e.shiftKey) {
      e.preventDefault();
      handleSend();
    }
  };

  if (!isOpen) return null;

  return createPortal(
    <div className="sensei-overlay" onClick={onClose}>
      <div className="sensei-panel" onClick={(e) => e.stopPropagation()}>
        {/* ── Header ──────────────────────────────────── */}
        <div className="sensei-header">
          <div className="sensei-header-left">
            <Sparkles size={18} className="sensei-icon-sparkle" />
            <span className="sensei-title">Sensei 先生</span>
            {availableModels.length > 0 ? (
              <select
                className="sensei-model-select"
                value={activeModel || ""}
                onChange={(e) => handleModelChange(e.target.value)}
                disabled={isStreaming}
                title="Sélectionner le modèle LLM"
              >
                {availableModels.map((m) => (
                  <option key={m.name} value={m.name}>
                    {m.name} ({(m.size / 1e9).toFixed(1)} GB)
                  </option>
                ))}
              </select>
            ) : activeModel ? (
              <span className="sensei-model-badge">{activeModel}</span>
            ) : null}
            {isWarmingUp && (
              <span className="sensei-warmup-badge">
                <Loader2 size={12} className="sensei-spinner" />
                chargement…
              </span>
            )}
          </div>
          <button className="sensei-close" onClick={onClose} title="Fermer">
            <X size={18} />
          </button>
        </div>

        {/* ── File Context ────────────────────────────── */}
        <div className="sensei-context">
          <FileCode size={14} />
          <span className="sensei-context-file">{filename}</span>
          <span className="sensei-context-meta">
            {language ?? "text"} · {lineCount} lignes
          </span>
        </div>

        {/* ── Messages ────────────────────────────────── */}
        <div className="sensei-messages">
          {messages.length === 0 && (
            <div className="sensei-welcome">
              <div className="sensei-welcome-icon">🥷</div>
              <h3>Posez une question à Sensei</h3>
              <p>
                Je consulte <strong>Tensai</strong> (mémoire sémantique) et{" "}
                <strong>Oracle</strong> (qualité du code) pour vous guider.
              </p>

              {/* Quick Actions */}
              <div className="sensei-quick-actions">
                {quickActions.map((action) => (
                  <button
                    key={action.label}
                    className="sensei-quick-btn"
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
              className={`sensei-msg sensei-msg-${msg.role}`}
            >
              <div className="sensei-msg-content">
                {msg.role === "user" ? (
                  <p>{msg.content}</p>
                ) : msg.role === "system" ? (
                  <p className="sensei-msg-error">{msg.content}</p>
                ) : (
                  <>
                    <div
                      className="sensei-msg-md"
                      dangerouslySetInnerHTML={{
                        __html: simpleMarkdown(msg.content),
                      }}
                    />
                    {msg.isStreaming && (
                      <span className="sensei-cursor" />
                    )}

                    {/* Oracle Badge */}
                    {msg.oracleScore != null && (
                      <div className="sensei-oracle-badge">
                        <Shield size={12} />
                        <span
                          className={`sensei-oracle-score ${
                            msg.oracleScore >= 80
                              ? "sensei-score-good"
                              : msg.oracleScore >= 60
                                ? "sensei-score-mid"
                                : "sensei-score-low"
                          }`}
                        >
                          Oracle: {Math.round(msg.oracleScore)}/100
                        </span>
                        {msg.oracleSummary && (
                          <span className="sensei-oracle-summary-text">
                            {msg.oracleSummary.slice(0, 80)}
                            {msg.oracleSummary.length > 80 ? "…" : ""}
                          </span>
                        )}
                      </div>
                    )}

                    {/* RAG Sources */}
                    {msg.sources && msg.sources.length > 0 && !msg.isStreaming && (
                      <div className="sensei-sources">
                        <span className="sensei-sources-label">
                          📎 Sources Tensai :
                        </span>
                        {msg.sources.map((src, i) => (
                          <span key={i} className="sensei-source-chip">
                            {src.file_path.split("/").pop()}
                            {src.name ? `:${src.name}` : ""}
                            <span className="sensei-source-sim">
                              {Math.round(src.similarity * 100)}%
                            </span>
                          </span>
                        ))}
                      </div>
                    )}
                  </>
                )}
              </div>

              {/* Timestamp + Duration */}
              <div className="sensei-msg-footer">
                {msg.durationMs && !msg.isStreaming && (
                  <span className="sensei-msg-duration">
                    <Clock size={10} />
                    {(msg.durationMs / 1000).toFixed(1)}s
                  </span>
                )}
                <span className="sensei-msg-time">
                  {msg.timestamp.toLocaleTimeString("fr-FR", {
                    hour: "2-digit",
                    minute: "2-digit",
                  })}
                </span>
              </div>
            </div>
          ))}

          {isStreaming && messages.length > 0 && messages[messages.length - 1].content === "" && (
            <div className="sensei-msg sensei-msg-assistant">
              <div className="sensei-msg-loading">
                <Loader2 size={16} className="sensei-spinner" />
                <span>Sensei réfléchit…</span>
              </div>
            </div>
          )}

          <div ref={messagesEndRef} />
        </div>

        {/* ── Input ───────────────────────────────────── */}
        <div className="sensei-input-bar">
          <input
            ref={inputRef}
            type="text"
            className="sensei-input"
            placeholder={`Poser une question sur ${filename}…`}
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            onKeyDown={handleKeyDown}
            disabled={isStreaming}
          />
          {isStreaming ? (
            <button
              className="sensei-stop"
              onClick={handleStop}
              title="Arrêter la génération"
            >
              <StopCircle size={16} />
            </button>
          ) : (
            <button
              className="sensei-send"
              onClick={() => handleSend()}
              disabled={!query.trim()}
              title="Envoyer"
            >
              <Send size={16} />
            </button>
          )}
        </div>
      </div>
    </div>,
    document.body,
  );
}

// ── Simple Markdown → HTML (inline, no deps) ─────────────────
function simpleMarkdown(text: string): string {
  return text
    .replace(
      /```(\w+)?\n([\s\S]*?)```/g,
      '<pre class="sensei-code-block"><code>$2</code></pre>',
    )
    .replace(/`([^`]+)`/g, '<code class="sensei-inline-code">$1</code>')
    .replace(/\*\*([^*]+)\*\*/g, "<strong>$1</strong>")
    .replace(/\*([^*]+)\*/g, "<em>$1</em>")
    .replace(/## (.+)/g, '<h4 class="sensei-md-h2">$1</h4>')
    .replace(/### (.+)/g, '<h5 class="sensei-md-h3">$1</h5>')
    .replace(/\n/g, "<br/>");
}
