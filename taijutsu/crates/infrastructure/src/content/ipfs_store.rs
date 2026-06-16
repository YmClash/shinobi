//! Adaptateur Genjutsu — Stockage distribué via IPFS (Kubo).
//!
//! Utilise `reqwest` pour communiquer avec l'API HTTP RPC de Kubo
//! (port 5001 par défaut). Implémente le port `ContentStore`.
//!
//! ## Architecture
//! - `reqwest::Client` : client HTTP async réutilisable (connection pooling).
//! - Les contenus sont adressés par CID (Content Identifier) — format IPFS.
//! - Chaque méthode mappe 1:1 vers un endpoint Kubo `/api/v0/*`.
//!
//! ## Endpoints Kubo utilisés
//! | Méthode       | Endpoint                          |
//! |---------------|-----------------------------------|
//! | `store()`     | `POST /api/v0/add`                |
//! | `retrieve()`  | `POST /api/v0/cat?arg=<CID>`      |
//! | `exists()`    | `POST /api/v0/block/stat?arg=<CID>`|
//! | `pin()`       | `POST /api/v0/pin/add?arg=<CID>`  |
//!
//! ## Gestion d'erreurs
//! Toutes les erreurs HTTP/réseau sont mappées vers `DomainError::StorageError`.
//! Le client est construit une seule fois (lazy connection pooling).

use async_trait::async_trait;
use reqwest::multipart;
use serde::Deserialize;
use tracing::{info, warn};

use domain::entities::content_id::ContentId;
use domain::errors::DomainError;
use domain::ports::content_store::ContentStore;

/// Réponse JSON de Kubo pour `POST /api/v0/add`.
///
/// Kubo retourne un objet JSON par fichier ajouté.
/// On ne s'intéresse qu'au `Hash` (CID) et au `Size`.
#[derive(Debug, Deserialize)]
struct IpfsAddResponse {
    /// CID du contenu ajouté (ex: "QmXy..." ou "bafk...")
    #[serde(rename = "Hash")]
    hash: String,

    /// Taille du contenu en bytes (string dans la réponse Kubo)
    #[serde(rename = "Size")]
    #[allow(dead_code)]
    size: String,
}

/// Client de stockage distribué IPFS (Genjutsu).
///
/// Encapsule un `reqwest::Client` configuré pour communiquer avec
/// le nœud Kubo local via son API HTTP RPC.
///
/// Thread-safe (`Send + Sync`) par conception — `reqwest::Client`
/// utilise un pool de connexions interne partageable via `Arc`.
pub struct IpfsContentStore {
    client: reqwest::Client,
    /// URL de base de l'API RPC Kubo (ex: `"http://127.0.0.1:5001"`)
    api_url: String,
}

impl IpfsContentStore {
    /// Crée un nouveau client IPFS.
    ///
    /// # Arguments
    /// - `api_url` : URL de l'API RPC Kubo (ex: `"http://127.0.0.1:5001"`)
    ///
    /// # Note
    /// La connexion est **lazy** : aucune requête n'est envoyée à la construction.
    /// La première erreur réseau apparaîtra lors du premier appel `store()/retrieve()`.
    pub fn new(api_url: &str) -> Result<Self, DomainError> {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .map_err(|e| {
                DomainError::StorageError(format!("IPFS HTTP client creation failed: {e}"))
            })?;

        info!(
            api_url = %api_url,
            "IpfsContentStore initialisé (connexion lazy)"
        );

        Ok(Self {
            client,
            api_url: api_url.trim_end_matches('/').to_string(),
        })
    }
}

#[async_trait]
impl ContentStore for IpfsContentStore {
    /// Stocke un blob de données sur IPFS et retourne son CID.
    ///
    /// Utilise `POST /api/v0/add` avec un body `multipart/form-data`.
    /// Kubo hashe le contenu (SHA-256 par défaut) et retourne le CID.
    async fn store(&self, data: &[u8]) -> Result<ContentId, DomainError> {
        let url = format!("{}/api/v0/add", self.api_url);

        // Construire le multipart form avec le blob en tant que fichier
        let part = multipart::Part::bytes(data.to_vec())
            .file_name("blob")
            .mime_str("application/octet-stream")
            .map_err(|e| {
                DomainError::StorageError(format!("Multipart MIME error: {e}"))
            })?;

        let form = multipart::Form::new().part("file", part);

        let response = self
            .client
            .post(&url)
            .multipart(form)
            .send()
            .await
            .map_err(|e| {
                DomainError::StorageError(format!("IPFS add request failed: {e}"))
            })?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(DomainError::StorageError(format!(
                "IPFS add failed (HTTP {status}): {body}"
            )));
        }

        let add_response: IpfsAddResponse = response.json().await.map_err(|e| {
            DomainError::StorageError(format!("IPFS add response parse failed: {e}"))
        })?;

        let cid = ContentId::new(&add_response.hash);

        info!(
            cid = %cid,
            "✅ Contenu stocké sur IPFS (Genjutsu)"
        );

        Ok(cid)
    }

    /// Récupère un blob par son CID depuis IPFS.
    ///
    /// Utilise `POST /api/v0/cat?arg=<CID>`.
    /// Le contenu est streamé en mémoire et retourné comme `Vec<u8>`.
    async fn retrieve(&self, cid: &ContentId) -> Result<Vec<u8>, DomainError> {
        let url = format!("{}/api/v0/cat?arg={}", self.api_url, cid.as_str());

        let response = self
            .client
            .post(&url)
            .send()
            .await
            .map_err(|e| {
                DomainError::StorageError(format!("IPFS cat request failed: {e}"))
            })?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(DomainError::StorageError(format!(
                "IPFS cat failed (HTTP {status}): {body}"
            )));
        }

        let bytes = response.bytes().await.map_err(|e| {
            DomainError::StorageError(format!("IPFS cat response read failed: {e}"))
        })?;

        info!(
            cid = %cid,
            size = bytes.len(),
            "✅ Contenu récupéré depuis IPFS (Genjutsu)"
        );

        Ok(bytes.to_vec())
    }

    /// Vérifie l'existence d'un contenu sans le télécharger.
    ///
    /// Utilise `POST /api/v0/block/stat?arg=<CID>`.
    /// Retourne `true` si le bloc existe, `false` sinon.
    async fn exists(&self, cid: &ContentId) -> Result<bool, DomainError> {
        let url = format!("{}/api/v0/block/stat?arg={}", self.api_url, cid.as_str());

        let response = self
            .client
            .post(&url)
            .send()
            .await
            .map_err(|e| {
                DomainError::StorageError(format!("IPFS block/stat request failed: {e}"))
            })?;

        let exists = response.status().is_success();

        if exists {
            info!(cid = %cid, "IPFS block exists");
        } else {
            warn!(cid = %cid, "IPFS block not found");
        }

        Ok(exists)
    }

    /// Épingle un contenu pour empêcher sa collecte par le garbage collector.
    ///
    /// Utilise `POST /api/v0/pin/add?arg=<CID>`.
    /// Une fois épinglé, le contenu persiste même sans demandes de lecture.
    async fn pin(&self, cid: &ContentId) -> Result<(), DomainError> {
        let url = format!("{}/api/v0/pin/add?arg={}", self.api_url, cid.as_str());

        let response = self
            .client
            .post(&url)
            .send()
            .await
            .map_err(|e| {
                DomainError::StorageError(format!("IPFS pin/add request failed: {e}"))
            })?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(DomainError::StorageError(format!(
                "IPFS pin/add failed (HTTP {status}): {body}"
            )));
        }

        info!(
            cid = %cid,
            "📌 Contenu épinglé sur IPFS (Genjutsu)"
        );

        Ok(())
    }
}

// ── Tests d'intégration (nécessitent Kubo Docker) ─────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    /// Helper : crée un client connecté au Kubo local.
    fn test_store() -> IpfsContentStore {
        IpfsContentStore::new("http://127.0.0.1:5001")
            .expect("Failed to create IpfsContentStore for tests")
    }

    #[tokio::test]
    #[ignore] // Nécessite: docker compose up -d (Kubo running on :5001)
    async fn test_ipfs_store_returns_valid_cid() {
        let store = test_store();
        let data = b"Hello SHINOBI from Genjutsu!";

        let cid = store.store(data).await;
        assert!(cid.is_ok(), "store() failed: {:?}", cid.err());

        let cid = cid.unwrap();
        let cid_str = cid.to_string();
        assert!(!cid_str.is_empty(), "CID should not be empty");
        // IPFS CIDv0 commence par "Qm", CIDv1 par "bafy"
        assert!(
            cid_str.starts_with("Qm") || cid_str.starts_with("bafy"),
            "CID should start with 'Qm' or 'bafy', got: {cid_str}"
        );
    }

    #[tokio::test]
    #[ignore] // Nécessite: docker compose up -d
    async fn test_ipfs_store_and_retrieve_roundtrip() {
        let store = test_store();
        let original_data = b"SHINOBI Genjutsu roundtrip test data - 2026";

        // Store
        let cid = store.store(original_data).await.unwrap();

        // Retrieve
        let retrieved = store.retrieve(&cid).await;
        assert!(
            retrieved.is_ok(),
            "retrieve() failed: {:?}",
            retrieved.err()
        );

        let retrieved_data = retrieved.unwrap();
        assert_eq!(
            retrieved_data, original_data,
            "Retrieved data should match original"
        );
    }

    #[tokio::test]
    #[ignore] // Nécessite: docker compose up -d
    async fn test_ipfs_exists_returns_true_after_store() {
        let store = test_store();
        let data = b"Existence check data";

        let cid = store.store(data).await.unwrap();

        let exists = store.exists(&cid).await;
        assert!(exists.is_ok(), "exists() failed: {:?}", exists.err());
        assert!(
            exists.unwrap(),
            "exists() should return true for stored content"
        );
    }

    #[tokio::test]
    #[ignore] // Nécessite: docker compose up -d
    async fn test_ipfs_exists_returns_false_for_unknown_cid() {
        let store = test_store();

        // CID valide syntaxiquement mais inexistant sur ce noeud
        let fake_cid = ContentId::new("QmFakeCIDThatDoesNotExistOnThisNode123456789abcdef");

        let exists = store.exists(&fake_cid).await;
        assert!(exists.is_ok(), "exists() failed: {:?}", exists.err());
        assert!(
            !exists.unwrap(),
            "exists() should return false for unknown CID"
        );
    }

    #[tokio::test]
    #[ignore] // Nécessite: docker compose up -d
    async fn test_ipfs_pin_after_store() {
        let store = test_store();
        let data = b"Pin this content on IPFS";

        let cid = store.store(data).await.unwrap();

        let pin_result = store.pin(&cid).await;
        assert!(
            pin_result.is_ok(),
            "pin() failed: {:?}",
            pin_result.err()
        );
    }
}
