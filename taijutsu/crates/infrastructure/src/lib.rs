//! # SHINOBI — Infrastructure Layer
//!
//! Adaptateurs secondaires de l'architecture hexagonale.
//! Chaque module implémente un Port (Trait) défini dans le domaine.
//!
//! ## Modules
//! - `persistence` — **Fūinjutsu** : PostgreSQL (via sqlx)
//! - `cache`       — **Fūinjutsu** : Redis (verrous distribués, cache)
//! - `vcs`         — Moteur VCS : jj-lib (Anti-Corruption Layer)
//! - `events`      — **Nen** : Kafka (publication événementielle)
//! - `content`     — **Genjutsu** : IPFS/Kubo (stockage distribué)

pub mod cache;
pub mod content;
pub mod events;
pub mod persistence;
pub mod vcs;
