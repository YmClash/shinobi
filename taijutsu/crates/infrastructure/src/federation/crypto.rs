//! Cryptographie RSA-2048 pour la fédération ActivityPub.
//!
//! Génère des paires de clés RSA-2048 au format PEM pour :
//! - Signer les requêtes HTTP sortantes (Draft-Cavage-12)
//! - Exposer la clé publique dans le profil ActivityPub

use rsa::RsaPrivateKey;
use rsa::pkcs8::{EncodePrivateKey, EncodePublicKey, LineEnding};
use tracing::info;

/// Erreur de génération cryptographique.
#[derive(Debug, thiserror::Error)]
pub enum CryptoError {
    #[error("RSA key generation failed: {0}")]
    KeyGeneration(String),
    #[error("PEM encoding failed: {0}")]
    PemEncoding(String),
}

/// Résultat de la génération de clés : (public_pem, private_pem).
pub struct RsaKeypair {
    pub public_key_pem: String,
    pub private_key_pem: String,
}

/// Génère une paire de clés RSA-2048 au format PEM.
///
/// Utilise le CSPRNG du système (OsRng) pour la génération.
/// Le format est PKCS#8 pour la clé privée, SPKI pour la publique.
pub fn generate_rsa_keypair() -> Result<RsaKeypair, CryptoError> {
    info!("🔐 Génération d'une paire de clés RSA-2048...");

    let mut rng = rsa::rand_core::OsRng;
    let private_key = RsaPrivateKey::new(&mut rng, 2048)
        .map_err(|e| CryptoError::KeyGeneration(e.to_string()))?;

    let private_pem = private_key
        .to_pkcs8_pem(LineEnding::LF)
        .map_err(|e| CryptoError::PemEncoding(e.to_string()))?;

    let public_pem = private_key
        .to_public_key()
        .to_public_key_pem(LineEnding::LF)
        .map_err(|e| CryptoError::PemEncoding(e.to_string()))?;

    info!("✅ Paire de clés RSA-2048 générée");

    Ok(RsaKeypair {
        public_key_pem: public_pem,
        private_key_pem: private_pem.to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_rsa_keypair() {
        let keypair = generate_rsa_keypair().expect("keypair generation should succeed");
        assert!(keypair.public_key_pem.contains("BEGIN PUBLIC KEY"));
        assert!(keypair.private_key_pem.contains("BEGIN PRIVATE KEY"));
    }

    #[test]
    fn test_keypair_unique() {
        let kp1 = generate_rsa_keypair().unwrap();
        let kp2 = generate_rsa_keypair().unwrap();
        assert_ne!(kp1.public_key_pem, kp2.public_key_pem);
    }
}
