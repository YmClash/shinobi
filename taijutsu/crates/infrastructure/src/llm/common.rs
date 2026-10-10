//! Utilitaires partagés par les adaptateurs LLM distants (Phase 42-pre).
//!
//! - Construction du client HTTP (timeouts explicites)
//! - Formatage des erreurs HTTP (body tronqué, jamais de secret)
//! - Politique de retry (429 / 5xx / transport uniquement)
//! - Métriques Prometheus homogènes (`model` + `provider`)

use std::time::{Duration, Instant};

use metrics::{counter, histogram};
use reqwest::StatusCode;
use tracing::info;

use domain::errors::DomainError;

/// Taille maximale du body d'erreur conservé dans les messages (anti-log-flood).
const MAX_ERROR_BODY: usize = 500;

/// Délai avant l'unique retry d'un appel non-streaming.
pub(crate) const RETRY_DELAY: Duration = Duration::from_secs(2);

/// Construit un client HTTP pour une API LLM distante.
pub(crate) fn build_client(provider: &str, timeout: Duration) -> Result<reqwest::Client, DomainError> {
    reqwest::Client::builder()
        .timeout(timeout)
        .connect_timeout(Duration::from_secs(10))
        .build()
        .map_err(|e| DomainError::Internal(format!("{provider} HTTP client creation failed: {e}")))
}

/// Erreur d'appel à un fournisseur, avec indication de « retryabilité ».
#[derive(Debug)]
pub(crate) struct CallError {
    pub message: String,
    pub retryable: bool,
}

impl From<CallError> for DomainError {
    fn from(e: CallError) -> Self {
        DomainError::Internal(e.message)
    }
}

/// Un statut justifie-t-il un retry ? (rate-limit ou erreur serveur)
pub(crate) fn is_retryable_status(status: StatusCode) -> bool {
    status == StatusCode::TOO_MANY_REQUESTS || status.is_server_error()
}

/// Erreur de transport (connexion, timeout) → toujours retryable.
pub(crate) fn transport_error(provider: &str, model: &str, e: reqwest::Error) -> CallError {
    CallError {
        message: format!("{provider} HTTP request failed ({model}): {e}"),
        retryable: true,
    }
}

/// Convertit une réponse non-2xx en `CallError` (body tronqué).
pub(crate) async fn http_error(provider: &str, response: reqwest::Response) -> CallError {
    let status = response.status();
    let body = response.text().await.unwrap_or_default();
    CallError {
        message: format!("{provider} API error (HTTP {status}): {}", truncate(&body)),
        retryable: is_retryable_status(status),
    }
}

/// Tronque un texte à `MAX_ERROR_BODY` caractères (frontière UTF-8 sûre).
pub(crate) fn truncate(s: &str) -> String {
    if s.chars().count() <= MAX_ERROR_BODY {
        s.to_string()
    } else {
        let cut: String = s.chars().take(MAX_ERROR_BODY).collect();
        format!("{cut}…")
    }
}

/// Enregistre les métriques d'une inférence réussie.
pub(crate) fn record_inference(provider: &str, model: &str, start: Instant) {
    histogram!(
        "llm_inference_duration_seconds",
        "model" => model.to_string(),
        "provider" => provider.to_string()
    )
    .record(start.elapsed().as_secs_f64());
    counter!(
        "llm_inference_total",
        "model" => model.to_string(),
        "provider" => provider.to_string()
    )
    .increment(1);
    info!(
        provider,
        model,
        duration_ms = start.elapsed().as_millis() as u64,
        "🧠 Hub LLM — inférence terminée"
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn retryable_statuses() {
        assert!(is_retryable_status(StatusCode::TOO_MANY_REQUESTS));
        assert!(is_retryable_status(StatusCode::BAD_GATEWAY));
        assert!(!is_retryable_status(StatusCode::UNAUTHORIZED));
        assert!(!is_retryable_status(StatusCode::BAD_REQUEST));
    }

    #[test]
    fn truncate_is_utf8_safe() {
        let long = "é".repeat(MAX_ERROR_BODY + 10);
        let t = truncate(&long);
        assert_eq!(t.chars().count(), MAX_ERROR_BODY + 1); // + '…'
    }
}
