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

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// ── Phase 27-bis-A — Vérification de signature HTTP entrante
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

/// Signature HTTP parsée (Draft-Cavage-12).
#[derive(Debug, Clone)]
pub struct ParsedSignature {
    /// `keyId` — URI de la clé publique du signataire.
    /// Ex: `https://mastodon.social/users/bob#main-key`
    pub key_id: String,
    /// `algorithm` — Algorithme de signature (ex: `rsa-sha256`).
    pub algorithm: String,
    /// `headers` — Liste des headers signés (ex: `["(request-target)", "host", "date", "digest"]`).
    pub headers: Vec<String>,
    /// `signature` — Signature Base64-encodée.
    pub signature_b64: String,
}

/// Parse le header `Signature` (Draft-Cavage-12).
///
/// ## Piège de la casse
/// Les noms de paramètres sont case-insensitive dans le draft, mais en
/// pratique Mastodon les envoie toujours en minuscules. On normalise.
///
/// Format attendu :
/// ```text
/// keyId="...",algorithm="rsa-sha256",headers="(request-target) host date digest",signature="..."
/// ```
pub fn parse_signature_header(header: &str) -> Result<ParsedSignature, HttpSignatureError> {
    let mut key_id = String::new();
    let mut algorithm = String::new();
    let mut headers_str = String::new();
    let mut signature_b64 = String::new();

    // Parse key=value pairs
    // Le parsing est volontairement robuste : on supporte les espaces
    // autour des virgules et les guillemets optionnels.
    for part in split_signature_params(header) {
        let part = part.trim();
        if let Some((key, value)) = part.split_once('=') {
            let key = key.trim().to_lowercase();  // ← Case normalization !
            let value = value.trim().trim_matches('"').to_string();
            match key.as_str() {
                "keyid" => key_id = value,
                "algorithm" => algorithm = value,
                "headers" => headers_str = value,
                "signature" => signature_b64 = value,
                _ => {} // Ignore les paramètres inconnus
            }
        }
    }

    if key_id.is_empty() {
        return Err(HttpSignatureError::MissingHeader("Signature.keyId".into()));
    }
    if signature_b64.is_empty() {
        return Err(HttpSignatureError::MissingHeader("Signature.signature".into()));
    }

    // Headers par défaut si non spécifiés
    let headers = if headers_str.is_empty() {
        vec!["date".to_string()]
    } else {
        // ← Case normalization des noms de headers !
        headers_str.split_whitespace().map(|h| h.to_lowercase()).collect()
    };

    Ok(ParsedSignature {
        key_id,
        algorithm,
        headers,
        signature_b64,
    })
}

/// Split les paramètres du header Signature en respectant les guillemets.
///
/// `keyId="a,b",signature="xyz"` → `["keyId=\"a,b\"", "signature=\"xyz\""]`
fn split_signature_params(header: &str) -> Vec<String> {
    let mut result = Vec::new();
    let mut current = String::new();
    let mut in_quotes = false;

    for c in header.chars() {
        match c {
            '"' => {
                in_quotes = !in_quotes;
                current.push(c);
            }
            ',' if !in_quotes => {
                result.push(current.trim().to_string());
                current = String::new();
            }
            _ => current.push(c),
        }
    }
    if !current.trim().is_empty() {
        result.push(current.trim().to_string());
    }
    result
}

/// Vérifie une signature HTTP entrante avec la clé publique RSA.
///
/// ## Arguments
/// - `public_key_pem` : Clé publique RSA de l'expéditeur (PEM)
/// - `signature_header` : Header `Signature` complet
/// - `method` : Méthode HTTP (`POST`)
/// - `path` : Chemin de la requête (`/actors/system/inbox`)
/// - `request_headers` : Map des headers de la requête (pour reconstruire le signing string)
///
/// ## Piège de la casse (Mastodon)
/// Tous les noms de headers sont convertis en minuscules avant de
/// reconstruire le signing string. Mastodon peut envoyer
/// `(request-target)` ou `(Request-Target)`.
pub fn verify_signature(
    public_key_pem: &str,
    signature_header: &str,
    method: &str,
    path: &str,
    request_headers: &std::collections::HashMap<String, String>,
) -> Result<ParsedSignature, HttpSignatureError> {
    use rsa::RsaPublicKey;
    use rsa::pkcs8::DecodePublicKey;
    use rsa::pkcs1v15::VerifyingKey;
    use rsa::signature::Verifier;

    // 1. Parser le header Signature
    let parsed = parse_signature_header(signature_header)?;

    // 2. Reconstruire le signing string dans l'ORDRE spécifié par `headers`
    let mut signing_parts = Vec::new();
    for header_name in &parsed.headers {
        let name_lower = header_name.to_lowercase();
        match name_lower.as_str() {
            "(request-target)" => {
                let request_target = format!("{} {}", method.to_lowercase(), path);
                signing_parts.push(format!("(request-target): {}", request_target));
            }
            _ => {
                // Chercher le header dans la map (case-insensitive)
                let value = request_headers
                    .iter()
                    .find(|(k, _)| k.to_lowercase() == name_lower)
                    .map(|(_, v)| v.as_str())
                    .ok_or_else(|| HttpSignatureError::MissingHeader(name_lower.clone()))?;
                signing_parts.push(format!("{}: {}", name_lower, value));
            }
        }
    }
    let signing_string = signing_parts.join("\n");

    // 3. Décoder la clé publique RSA
    let public_key = RsaPublicKey::from_public_key_pem(public_key_pem)
        .map_err(|e| HttpSignatureError::KeyParse(e.to_string()))?;

    // 4. Décoder la signature Base64
    let sig_bytes = base64::Engine::decode(
        &base64::engine::general_purpose::STANDARD,
        &parsed.signature_b64,
    ).map_err(|e| HttpSignatureError::SignFailed(format!("Base64 decode: {}", e)))?;

    let signature = rsa::pkcs1v15::Signature::try_from(sig_bytes.as_slice())
        .map_err(|e| HttpSignatureError::SignFailed(format!("Signature parse: {}", e)))?;

    // 5. Vérifier la signature RSA-PKCS1-v1_5 + SHA-256
    let verifying_key = VerifyingKey::<Sha256>::new(public_key);
    verifying_key.verify(signing_string.as_bytes(), &signature)
        .map_err(|_| HttpSignatureError::InvalidSignature)?;

    Ok(parsed)
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

    #[test]
    fn test_parse_signature_header() {
        let header = r#"keyId="https://mastodon.social/users/bob#main-key",algorithm="rsa-sha256",headers="(request-target) host date digest",signature="abc123==""#;
        let parsed = parse_signature_header(header).unwrap();
        assert_eq!(parsed.key_id, "https://mastodon.social/users/bob#main-key");
        assert_eq!(parsed.algorithm, "rsa-sha256");
        assert_eq!(parsed.headers, vec!["(request-target)", "host", "date", "digest"]);
        assert_eq!(parsed.signature_b64, "abc123==");
    }

    #[test]
    fn test_parse_signature_header_case_insensitive() {
        // Mastodon peut envoyer des noms de headers en casse variable
        let header = r#"keyId="https://example.com/key",headers="(Request-Target) Host Date Digest",signature="sig==""#;
        let parsed = parse_signature_header(header).unwrap();
        // Tous les headers doivent être en minuscules
        assert_eq!(parsed.headers, vec!["(request-target)", "host", "date", "digest"]);
    }

    #[test]
    fn test_parse_signature_header_missing_keyid() {
        let header = r#"algorithm="rsa-sha256",signature="abc123""#;
        assert!(parse_signature_header(header).is_err());
    }

    #[test]
    fn test_sign_and_verify_roundtrip() {
        let (pub_pem, priv_pem) = test_keypair();
        let body = b"{\"type\": \"Follow\", \"actor\": \"https://remote.example/user\"}";

        // Sign
        let sig_headers = sign_request(
            &priv_pem,
            "https://shinobi.dev/actors/system#main-key",
            "POST",
            "/actors/alice/inbox",
            "shinobi.dev",
            body,
        ).unwrap();

        // Build request headers map
        let mut request_headers = std::collections::HashMap::new();
        request_headers.insert("host".to_string(), "shinobi.dev".to_string());
        request_headers.insert("date".to_string(), sig_headers.date.clone());
        request_headers.insert("digest".to_string(), sig_headers.digest.clone());

        // Verify
        let result = verify_signature(
            &pub_pem,
            &sig_headers.signature,
            "POST",
            "/actors/alice/inbox",
            &request_headers,
        );
        assert!(result.is_ok(), "Signature verification failed: {:?}", result.err());
    }

    #[test]
    fn test_verify_tampered_body_fails() {
        let (pub_pem, priv_pem) = test_keypair();
        let body = b"{\"type\": \"Follow\"}";

        // Sign with correct body
        let sig_headers = sign_request(
            &priv_pem,
            "https://shinobi.dev/actors/system#main-key",
            "POST",
            "/actors/alice/inbox",
            "shinobi.dev",
            body,
        ).unwrap();

        // Tamper: use a DIFFERENT digest (simulates body tampering)
        let mut request_headers = std::collections::HashMap::new();
        request_headers.insert("host".to_string(), "shinobi.dev".to_string());
        request_headers.insert("date".to_string(), sig_headers.date.clone());
        request_headers.insert("digest".to_string(), "SHA-256=TAMPERED".to_string());

        // Verify should FAIL because digest changed
        let result = verify_signature(
            &pub_pem,
            &sig_headers.signature,
            "POST",
            "/actors/alice/inbox",
            &request_headers,
        );
        assert!(result.is_err());
    }
}

