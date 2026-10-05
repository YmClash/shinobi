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
//! - `jutsu_runner` : exécution Docker via bollard (→ Phase 40)
//! - `jutsu_consumer` : consommation d'événements pipeline Kafka (→ Phase 40)

pub mod kafka_consumer;
pub mod kafka_producer;
pub mod oracle_consumer;
pub mod chakra_producer;
pub mod chakra_consumer;
pub mod chakra_dispatcher;
pub mod chakra_retry;
pub mod jutsu_runner;
// jutsu_consumer lives in the main binary (src/) to avoid circular deps
// (it references both application and infrastructure)
