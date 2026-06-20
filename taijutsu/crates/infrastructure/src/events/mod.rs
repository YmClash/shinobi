//! Adaptateurs événementiels — Module Nen.
//!
//! Contient les implémentations concrètes des ports événementiels :
//! - `kafka_producer` : publication d'événements (→ `EventPublisher`)
//! - `kafka_consumer` : consommation d'événements (→ Agent Tensai)
//! - `oracle_consumer` : consommation analysis-complete (→ Agent Oracle)

pub mod kafka_consumer;
pub mod kafka_producer;
pub mod oracle_consumer;

