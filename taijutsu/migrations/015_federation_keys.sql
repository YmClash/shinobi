-- Phase 27 — ForgeFed: Keypairs RSA pour la fédération ActivityPub
--
-- Stocke les paires de clés RSA-2048 des acteurs pour :
-- - Signer les requêtes HTTP sortantes (Draft-Cavage-12)
-- - Exposer la clé publique dans le profil ActivityPub (publicKey)
-- - Vérifier les signatures des requêtes entrantes (Inbox)

CREATE TABLE IF NOT EXISTS federation_keys (
    actor_id UUID PRIMARY KEY REFERENCES actors(id) ON DELETE CASCADE,
    public_key_pem TEXT NOT NULL,
    private_key_pem TEXT NOT NULL,
    key_id TEXT NOT NULL UNIQUE,  -- ex: "https://shinobi.example.com/actors/ymclash#main-key"
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Index sur key_id pour le lookup rapide lors de la vérification des signatures
CREATE INDEX IF NOT EXISTS idx_federation_keys_key_id ON federation_keys(key_id);
