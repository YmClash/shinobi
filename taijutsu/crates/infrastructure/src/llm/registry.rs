//! Registre & routage des LLM par tâche (Phase 42-pre — Hub d'Agents 🧠🔌).
//!
//! Le « Mercenary Switch » : chaque tâche (Oracle, Sensei, Kage Bunshin) se voit
//! attribuer un `Arc<dyn LlmService>` selon une route globale (variables d'env).
//! Les use cases ne changent pas — ils reçoivent toujours un `Arc<dyn LlmService>`.
//!
//! ## Règles de résolution
//! | Route | Résultat |
//! |---|---|
//! | `ollama` | LLM local de la tâche |
//! | `gemini` / `openai` / `deepseek` / `vllm` | adaptateur distant (si configuré), sinon local + `warn!` |
//! | `auto` | premier distant configuré (gemini > openai > deepseek > vllm), sinon local |
//!
//! Si un distant est choisi, qu'un LLM local existe et que le repli est activé,
//! le distant est enveloppé dans un `FallbackLlmService` (distant → local).
//!
//! ⚠️ Confidentialité (V1) : une route distante envoie le code (diffs, logs CI,
//! fichiers) au fournisseur tiers, **dépôts privés inclus**. Garde-fou par dépôt
//! prévu en V2 (surcharge par repo).

use std::fmt;
use std::str::FromStr;
use std::sync::Arc;
use std::time::Duration;

use tracing::{info, warn};

use domain::ports::llm_service::LlmService;

use super::fallback_service::FallbackLlmService;
use super::gemini_service::GeminiService;
use super::openai_compatible_service::OpenAiCompatibleService;

/// Tâche consommatrice d'un LLM.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LlmTask {
    /// Code review asynchrone (Phase 9).
    Oracle,
    /// Chat conversationnel streaming (Phase 15).
    Sensei,
    /// Auto-healing CI (Phase 41) — gros contexte.
    KageBunshin,
}

impl fmt::Display for LlmTask {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Oracle => "oracle",
            Self::Sensei => "sensei",
            Self::KageBunshin => "kage_bunshin",
        })
    }
}

/// Route demandée pour une tâche.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LlmRoute {
    Ollama,
    Gemini,
    OpenAi,
    DeepSeek,
    Vllm,
    Auto,
}

impl FromStr for LlmRoute {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim().to_ascii_lowercase().as_str() {
            "ollama" | "local" => Ok(Self::Ollama),
            "gemini" => Ok(Self::Gemini),
            "openai" => Ok(Self::OpenAi),
            "deepseek" => Ok(Self::DeepSeek),
            "vllm" | "lmstudio" | "lm-studio" => Ok(Self::Vllm),
            "auto" => Ok(Self::Auto),
            other => Err(format!(
                "route LLM inconnue '{other}' (attendu: ollama|gemini|openai|deepseek|vllm|auto)"
            )),
        }
    }
}

impl fmt::Display for LlmRoute {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Ollama => "ollama",
            Self::Gemini => "gemini",
            Self::OpenAi => "openai",
            Self::DeepSeek => "deepseek",
            Self::Vllm => "vllm",
            Self::Auto => "auto",
        })
    }
}

/// Ordre de préférence pour la route `auto`.
const AUTO_PRIORITY: [LlmRoute; 4] = [LlmRoute::Gemini, LlmRoute::OpenAi, LlmRoute::DeepSeek, LlmRoute::Vllm];

/// Paramètres d'un fournisseur distant (déjà validés par la config).
///
/// ⚠️ Pas de `Debug` : contient une clé d'API.
#[derive(Clone)]
pub struct RemoteProviderConfig {
    pub api_key: Option<String>,
    pub model: String,
    pub base_url: String,
}

/// Configuration du Hub d'Agents (construite depuis `config.rs`).
#[derive(Clone)]
pub struct LlmHubConfig {
    pub gemini: Option<RemoteProviderConfig>,
    pub openai: Option<RemoteProviderConfig>,
    pub deepseek: Option<RemoteProviderConfig>,
    pub vllm: Option<RemoteProviderConfig>,
    pub fallback_enabled: bool,
    pub external_timeout: Duration,
}

/// Registre des adaptateurs distants disponibles.
pub struct LlmRegistry {
    remotes: Vec<(LlmRoute, Arc<dyn LlmService>)>,
    fallback_enabled: bool,
}

impl LlmRegistry {
    /// Construit les adaptateurs distants configurés. Un adaptateur qui échoue
    /// à s'initialiser est ignoré avec un `warn!` (jamais bloquant).
    pub fn from_config(cfg: &LlmHubConfig) -> Self {
        let mut remotes: Vec<(LlmRoute, Arc<dyn LlmService>)> = Vec::new();
        let timeout = cfg.external_timeout;

        if let Some(g) = &cfg.gemini {
            match GeminiService::new(g.api_key.as_deref().unwrap_or_default(), &g.model, &g.base_url, timeout) {
                Ok(s) => remotes.push((LlmRoute::Gemini, Arc::new(s))),
                Err(e) => warn!(error = %e, "⚠️ Hub LLM — Gemini ignoré"),
            }
        }

        let openai_like = [
            (LlmRoute::OpenAi, "openai", &cfg.openai),
            (LlmRoute::DeepSeek, "deepseek", &cfg.deepseek),
            (LlmRoute::Vllm, "vllm", &cfg.vllm),
        ];
        for (route, label, provider) in openai_like {
            if let Some(p) = provider {
                match OpenAiCompatibleService::new(label, &p.base_url, &p.model, p.api_key.as_deref(), timeout) {
                    Ok(s) => remotes.push((route, Arc::new(s))),
                    Err(e) => warn!(provider = label, error = %e, "⚠️ Hub LLM — fournisseur ignoré"),
                }
            }
        }

        info!(
            providers = ?remotes.iter().map(|(r, _)| r.to_string()).collect::<Vec<_>>(),
            fallback = cfg.fallback_enabled,
            "🧠 Hub d'Agents — fournisseurs distants disponibles"
        );

        Self { remotes, fallback_enabled: cfg.fallback_enabled }
    }

    /// Constructeur direct (tests / injection).
    pub fn with_remotes(remotes: Vec<(LlmRoute, Arc<dyn LlmService>)>, fallback_enabled: bool) -> Self {
        Self { remotes, fallback_enabled }
    }

    /// Fournisseurs distants disponibles.
    pub fn available(&self) -> Vec<LlmRoute> {
        self.remotes.iter().map(|(r, _)| *r).collect()
    }

    fn remote(&self, route: LlmRoute) -> Option<Arc<dyn LlmService>> {
        self.remotes.iter().find(|(r, _)| *r == route).map(|(_, s)| s.clone())
    }

    /// Résout le LLM d'une tâche.
    ///
    /// - `route` : route demandée
    /// - `local` : LLM Ollama de la tâche (`None` si indisponible/désactivé)
    pub fn resolve(
        &self,
        task: LlmTask,
        route: LlmRoute,
        local: Option<Arc<dyn LlmService>>,
    ) -> Option<Arc<dyn LlmService>> {
        let remote = match route {
            LlmRoute::Ollama => None,
            LlmRoute::Auto => AUTO_PRIORITY.iter().find_map(|r| self.remote(*r)),
            specific => {
                let r = self.remote(specific);
                if r.is_none() {
                    warn!(
                        %task, %route,
                        "⚠️ Hub LLM — fournisseur demandé non configuré, repli sur Ollama"
                    );
                }
                r
            }
        };

        let resolved: Option<Arc<dyn LlmService>> = match (remote, local) {
            (Some(remote), Some(local)) if self.fallback_enabled => {
                Some(Arc::new(FallbackLlmService::new(remote, local)))
            }
            (Some(remote), _) => Some(remote),
            (None, local) => local,
        };

        match &resolved {
            Some(llm) => info!(
                %task, %route,
                provider = %llm.provider(),
                model = %llm.model_name(),
                "🧠 Hub LLM — tâche routée"
            ),
            None => warn!(%task, %route, "⚠️ Hub LLM — aucun LLM disponible pour cette tâche"),
        }

        resolved
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use domain::errors::DomainError;
    use domain::ports::llm_service::{LlmResponse, LlmStreamChunk};
    use tokio::sync::mpsc;

    struct Named(&'static str);

    #[async_trait]
    impl LlmService for Named {
        async fn generate(&self, _p: &str, _s: &str) -> Result<LlmResponse, DomainError> {
            unimplemented!()
        }
        async fn generate_stream(
            &self,
            _p: &str,
            _s: &str,
        ) -> Result<mpsc::Receiver<LlmStreamChunk>, DomainError> {
            unimplemented!()
        }
        fn model_name(&self) -> &str {
            self.0
        }
        fn provider(&self) -> &str {
            self.0
        }
    }

    fn local() -> Option<Arc<dyn LlmService>> {
        Some(Arc::new(Named("ollama")))
    }

    fn reg(remotes: &[LlmRoute], fallback: bool) -> LlmRegistry {
        let names = |r: &LlmRoute| -> &'static str {
            match r {
                LlmRoute::Gemini => "gemini",
                LlmRoute::OpenAi => "openai",
                LlmRoute::DeepSeek => "deepseek",
                LlmRoute::Vllm => "vllm",
                _ => "?",
            }
        };
        LlmRegistry::with_remotes(
            remotes.iter().map(|r| (*r, Arc::new(Named(names(r))) as Arc<dyn LlmService>)).collect(),
            fallback,
        )
    }

    #[test]
    fn parses_routes() {
        assert_eq!("GEMINI".parse::<LlmRoute>().unwrap(), LlmRoute::Gemini);
        assert_eq!(" lmstudio ".parse::<LlmRoute>().unwrap(), LlmRoute::Vllm);
        assert!("claude".parse::<LlmRoute>().is_err());
    }

    #[test]
    fn ollama_route_returns_local() {
        let r = reg(&[LlmRoute::Gemini], true);
        let llm = r.resolve(LlmTask::Sensei, LlmRoute::Ollama, local()).unwrap();
        assert_eq!(llm.provider(), "ollama");
    }

    #[test]
    fn auto_without_remote_returns_local() {
        let r = reg(&[], true);
        let llm = r.resolve(LlmTask::KageBunshin, LlmRoute::Auto, local()).unwrap();
        assert_eq!(llm.provider(), "ollama");
    }

    #[test]
    fn auto_respects_priority() {
        let r = reg(&[LlmRoute::Vllm, LlmRoute::OpenAi], false);
        let llm = r.resolve(LlmTask::KageBunshin, LlmRoute::Auto, local()).unwrap();
        assert_eq!(llm.provider(), "openai");
    }

    #[test]
    fn missing_specific_provider_falls_back_to_local() {
        let r = reg(&[LlmRoute::OpenAi], true);
        let llm = r.resolve(LlmTask::Oracle, LlmRoute::Gemini, local()).unwrap();
        assert_eq!(llm.provider(), "ollama");
    }

    #[test]
    fn remote_is_wrapped_with_fallback_but_reports_remote_identity() {
        let r = reg(&[LlmRoute::Gemini], true);
        let llm = r.resolve(LlmTask::KageBunshin, LlmRoute::Gemini, local()).unwrap();
        assert_eq!(llm.provider(), "gemini");
        assert_eq!(llm.model_name(), "gemini");
    }

    #[test]
    fn remote_without_local_still_works() {
        let r = reg(&[LlmRoute::DeepSeek], true);
        let llm = r.resolve(LlmTask::KageBunshin, LlmRoute::DeepSeek, None).unwrap();
        assert_eq!(llm.provider(), "deepseek");
    }

    #[test]
    fn nothing_available_returns_none() {
        let r = reg(&[], true);
        assert!(r.resolve(LlmTask::Sensei, LlmRoute::Auto, None).is_none());
    }
}
