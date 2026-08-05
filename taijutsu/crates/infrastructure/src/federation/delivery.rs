//! Service de livraison d'activités ActivityPub — Phase 27-bis-B.
//!
//! Envoie des activités signées (Accept, etc.) vers les inboxes distants.
//!
//! ## Architecture
//! - Utilisé via `tokio::spawn` pour ne pas bloquer la réponse Inbox
//! - Signe chaque requête avec Draft-Cavage-12 (RSA-SHA256)
//! - Timeout strict (5s) pour éviter les blocages
//!
//! ## Dette Technique Documentée
//! Si le serveur crashe pendant le `tokio::spawn`, l'activité est perdue.
//! V2 : job queue PostgreSQL/Redis avec retry exponentiel.

use tracing::{info, warn};

use super::http_signature::sign_request;

/// Envoie une activité signée vers un inbox distant.
///
/// ## Arguments
/// - `activity_json` : Activité AP sérialisée en JSON
/// - `target_inbox` : URL de l'inbox distant (ex: `https://mastodon.social/users/bob/inbox`)
/// - `private_key_pem` : Clé privée RSA de l'acteur local (PEM PKCS#8)
/// - `key_id` : Identifiant de la clé (ex: `https://shinobi.dev/actors/system#main-key`)
pub async fn deliver_activity(
    activity_json: serde_json::Value,
    target_inbox: &str,
    private_key_pem: &str,
    key_id: &str,
) -> Result<(), DeliveryError> {
    // Parse l'URL cible pour extraire host + path
    let parsed = url::Url::parse(target_inbox)
        .map_err(|e| DeliveryError::InvalidTarget(e.to_string()))?;

    let host = parsed.host_str()
        .ok_or_else(|| DeliveryError::InvalidTarget("No host".into()))?;
    let path = parsed.path();

    // Sérialiser le body
    let body = serde_json::to_vec(&activity_json)
        .map_err(|e| DeliveryError::SerializeFailed(e.to_string()))?;

    // Signer la requête
    let sig_headers = sign_request(private_key_pem, key_id, "POST", path, host, &body)
        .map_err(|e| DeliveryError::SignFailed(e.to_string()))?;

    // Envoyer la requête signée
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(5))
        .build()
        .map_err(|e| DeliveryError::HttpError(e.to_string()))?;

    info!(
        target = %target_inbox,
        key_id = %key_id,
        "📤 Delivering signed activity"
    );

    let response = client
        .post(target_inbox)
        .header("Content-Type", "application/activity+json")
        .header("Date", &sig_headers.date)
        .header("Digest", &sig_headers.digest)
        .header("Signature", &sig_headers.signature)
        .header("Host", host)
        .body(body)
        .send()
        .await
        .map_err(|e| {
            warn!(target = %target_inbox, error = %e, "❌ Delivery failed");
            DeliveryError::HttpError(e.to_string())
        })?;

    let status = response.status();
    if status.is_success() || status.as_u16() == 202 {
        info!(target = %target_inbox, status = %status, "✅ Activity delivered");
        Ok(())
    } else {
        let body_text = response.text().await.unwrap_or_default();
        warn!(
            target = %target_inbox,
            status = %status,
            body = %body_text,
            "⚠️ Delivery returned non-success"
        );
        // On ne retourne pas d'erreur pour 4xx — le serveur distant a bien reçu
        // mais ne veut pas l'activité (ex: acteur supprimé, inbox plein)
        Ok(())
    }
}

/// Erreurs de livraison.
#[derive(Debug, thiserror::Error)]
pub enum DeliveryError {
    #[error("Invalid target inbox URL: {0}")]
    InvalidTarget(String),

    #[error("Failed to serialize activity: {0}")]
    SerializeFailed(String),

    #[error("Failed to sign request: {0}")]
    SignFailed(String),

    #[error("HTTP delivery error: {0}")]
    HttpError(String),
}
