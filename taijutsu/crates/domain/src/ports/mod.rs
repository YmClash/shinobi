//! Ports du domaine — Contrats que les adaptateurs doivent implémenter.
//!
//! Chaque Trait ici est un **Port** au sens de l'architecture hexagonale :
//! - Les adaptateurs secondaires (infrastructure/) les **implémentent**.
//! - Les adaptateurs primaires (presentation/) les **consomment** via les use cases.

pub mod actor_repository;
pub mod auth_service;
pub mod chunk_repository;
pub mod content_store;
pub mod embedding_service;
pub mod event_consumer;
pub mod event_publisher;
pub mod github_service;
pub mod llm_service;
pub mod repo_repository;
pub mod repository;
pub mod review_repository;
pub mod vcs_engine;
