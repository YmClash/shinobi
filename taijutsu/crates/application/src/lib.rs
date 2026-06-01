//! # SHINOBI — Application Layer
//!
//! Couche d'orchestration des cas d'usage.
//! Chaque use case consomme les Ports (Traits) du domaine
//! via injection de dépendances, sans jamais connaître
//! les implémentations concrètes.

pub mod use_cases;
