//! Module LLM — Adaptateurs pour les modèles de langage.
//!
//! ## Phase 42-pre — Hub d'Agents 🧠🔌
//! - `ollama_service` : LLM local (Ollama)
//! - `gemini_service` : Google Gemini
//! - `openai_compatible_service` : OpenAI, DeepSeek, vLLM, LM Studio
//! - `fallback_service` : décorateur distant → local
//! - `registry` : routage par tâche (Oracle, Sensei, Kage Bunshin)

pub(crate) mod common;
pub mod fallback_service;
pub mod gemini_service;
pub mod ollama_service;
pub mod openai_compatible_service;
pub mod registry;
pub mod sse;
