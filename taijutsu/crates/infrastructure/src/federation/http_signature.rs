//! HTTP Signatures — Draft-Cavage-12 (de facto Fediverse standard).
//!
//! Implémente la signature et vérification des requêtes HTTP pour
//! la fédération ActivityPub (server-to-server).
//!
//! ## Piège de l'Horloge (Replay Attacks)
//! La vérification inclut un contrôle de fraîcheur du header `Date` :
//! les requêtes avec plus de 30 secondes d'écart sont rejetées.

use chrono::Utc;
use rsa::RsaPrivateKey;
use rsa::pkcs8::DecodePrivateKey;
use rsa::signature::SignatureEncoding;
use sha2::{Sha256, Digest};
use tracing::warn;

/// Headers de signature HTTP à ajouter à la requête sortante.
#[derive(Debug, Clone)]
pub struct SignatureHeaders {
    /// Header `Date` au format HTTP (RFC 2616).
    pub date: String,
    /// Header `Digest` (SHA-256 du body).
    pub digest: String,
    /// Header `Signature` complet (Draft-Cavage-12).
    pub signature: String,
}

/// Erreur de signature HTTP.
#[derive(Debug, thiserror::Error)]
pub enum HttpSignatureError {
    #[error("Failed to parse private key: {0}")]
    KeyParse(String),
    #[error("Signature generation failed: {0}")]
    SignFailed(String),
    #[error("Clock skew too large: {0}s (max 30s)")]
    ClockSkew(i64),
    #[error("Missing required header: {0}")]
    MissingHeader(String),
    #[error("Invalid signature")]
    InvalidSignature,
}

/// Signe une requête HTTP sortante avec la clé privée RSA.
///
/// Génère les headers `Date`, `Digest` et `Signature` conformes au
/// Draft-Cavage-12.
///
/// ## Arguments
/// - `private_key_pem` : Clé privée RSA au format PEM (PKCS#8)
/// - `key_id` : Identifiant de la clé (ex: `https://domain/actors/handle#main-key`)
/// - `method` : Méthode HTTP (POST, GET)
/// - `path` : Chemin de la requête (ex: `/actors/alice/inbox`)
/// - `host` : Hostname de la cible (ex: `forgejo.example.com`)
/// - `body` : Corps de la requête (vide pour GET)
pub fn sign_request(
    private_key_pem: &str,
    key_id: &str,
    method: &str,
    path: &str,
    host: &str,
    body: &[u8],
) -> Result<SignatureHeaders, HttpSignatureError> {
    use rsa::pkcs1v15::SigningKey;
    use rsa::signature::Signer;

    let private_key = RsaPrivateKey::from_pkcs8_pem(private_key_pem)
        .map_err(|e| HttpSignatureError::KeyParse(e.to_string()))?;

    // Date au format HTTP
    let date = Utc::now().format("%a, %d %b %Y %H:%M:%S GMT").to_string();

    // Digest du body (SHA-256)
    let body_hash = Sha256::digest(body);
    let digest = format!("SHA-256={}", base64::Engine::encode(
        &base64::engine::general_purpose::STANDARD, body_hash
    ));

    // Signing string (Draft-Cavage-12)
    let request_target = format!("{} {}", method.to_lowercase(), path);
    let signing_string = format!(
        "(request-target): {}\nhost: {}\ndate: {}\ndigest: {}",
        request_target, host, date, digest
    );

    // Sign with RSA-PKCS1-v1_5 + SHA-256
    let signing_key = SigningKey::<Sha256>::new(private_key);
    let sig = signing_key.sign(signing_string.as_bytes());
    let sig_b64 = base64::Engine::encode(
        &base64::engine::general_purpose::STANDARD, sig.to_bytes()
    );

    // Assemble le header Signature
    let signature_header = format!(
        "keyId=\"{}\",algorithm=\"rsa-sha256\",headers=\"(request-target) host date digest\",signature=\"{}\"",
        key_id, sig_b64
    );

    Ok(SignatureHeaders {
        date,
        digest,
        signature: signature_header,
    })
}

/// Vérifie la fraîcheur du header `Date` d'une requête entrante.
///
/// ## Piège de l'Horloge (Replay Attacks)
/// Rejette les requêtes dont le header `Date` a plus de 30 secondes
/// d'écart avec l'horloge du serveur. Sans ce contrôle, un attaquant
/// pourrait rejouer des requêtes interceptées indéfiniment.
pub fn verify_clock_skew(date_header: &str) -> Result<(), HttpSignatureError> {
    // Parse le format HTTP date (RFC 2616)
    let parsed = chrono::DateTime::parse_from_str(date_header, "%a, %d %b %Y %H:%M:%S GMT")
        .or_else(|_| chrono::DateTime::parse_from_rfc2822(date_header));

    match parsed {
        Ok(request_time) => {
            let skew = (Utc::now() - request_time.to_utc()).num_seconds().abs();
            if skew > 30 {
                warn!(
                    skew_seconds = skew,
                    date = %date_header,
                    "⚠️ Requête fédérée rejetée — clock skew trop important"
                );
                Err(HttpSignatureError::ClockSkew(skew))
            } else {
                Ok(())
            }
        }
        Err(_) => {
            warn!(date = %date_header, "⚠️ Header Date invalide dans la requête fédérée");
            Err(HttpSignatureError::MissingHeader("Date (invalid format)".into()))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_keypair() -> (String, String) {
        let kp = crate::federation::crypto::generate_rsa_keypair().unwrap();
        (kp.public_key_pem, kp.private_key_pem)
    }

    #[test]
    fn test_sign_request_roundtrip() {
        let (_pub_pem, priv_pem) = test_keypair();
        let result = sign_request(
            &priv_pem,
            "https://shinobi.example.com/actors/test#main-key",
            "POST",
            "/actors/alice/inbox",
            "forgejo.example.com",
            b"{\"type\": \"Follow\"}",
        );
        assert!(result.is_ok());
        let headers = result.unwrap();
        assert!(headers.signature.contains("keyId="));
        assert!(headers.signature.contains("rsa-sha256"));
        assert!(headers.digest.starts_with("SHA-256="));
        assert!(!headers.date.is_empty());
    }

    #[test]
    fn test_clock_skew_valid() {
        let now = Utc::now().format("%a, %d %b %Y %H:%M:%S GMT").to_string();
        assert!(verify_clock_skew(&now).is_ok());
    }

    #[test]
    fn test_clock_skew_too_old() {
        // 5 minutes ago → should be rejected
        let old = (Utc::now() - chrono::Duration::minutes(5))
            .format("%a, %d %b %Y %H:%M:%S GMT")
            .to_string();
        assert!(verify_clock_skew(&old).is_err());
    }
}
