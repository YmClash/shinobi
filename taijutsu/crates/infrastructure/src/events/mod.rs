//! Adaptateurs événementiels — Module Nen.
//!
//! Contient les implémentations concrètes des ports événementiels :
//! - `kafka_producer` : publication d'événements (→ `EventPublisher`)
//! - `kafka_consumer` : consommation d'événements (→ Agent Tensai)
//! - `oracle_consumer` : consommation analysis-complete (→ Agent Oracle)
//! - `chakra_producer` : publication d'événements webhook (→ Phase 34)
//! - `chakra_consumer` : consommation d'événements webhook (→ Phase 34)
//! - `chakra_dispatcher` : dispatch HTTP signé + SSRF protection (→ Phase 34)
//! - `chakra_retry` : retry worker Redis backoff exponentiel (→ Phase 34)

pub mod kafka_consumer;
pub mod kafka_producer;
pub mod oracle_consumer;
pub mod chakra_producer;
pub mod chakra_consumer;
pub mod chakra_dispatcher;
pub mod chakra_retry;
