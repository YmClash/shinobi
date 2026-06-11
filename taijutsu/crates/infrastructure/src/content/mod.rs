//! Adaptateurs de stockage distribué — Module Genjutsu.
//!
//! Contient les implémentations concrètes du port `ContentStore`.
//! L'adaptateur principal utilise IPFS (Kubo) via son API HTTP RPC.

pub mod ipfs_store;
