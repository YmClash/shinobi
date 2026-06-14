//! Ports du domaine — Contrats que les adaptateurs doivent implémenter.
//!
//! Chaque Trait ici est un **Port** au sens de l'architecture hexagonale :
//! - Les adaptateurs secondaires (infrastructure/) les **implémentent**.
//! - Les adaptateurs primaires (presentation/) les **consomment** via les use cases.

pub mod chunk_repository;
pub mod content_store;
pub mod event_consumer;
pub mod event_publisher;
pub mod repository;
pub mod vcs_engine;
