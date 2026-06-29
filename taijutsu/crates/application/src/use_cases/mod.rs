//! Use Cases — Points d'entrée de la logique métier.

pub mod analyze_operation;
pub mod create_operation;
pub mod get_operation;
pub mod get_operation_diff;
pub mod get_ipfs_content;
pub mod get_reviews;
pub mod get_score_history;
pub mod list_operations;
pub mod resolve_ipfs;
pub mod resolve_repo;
pub mod review_operation;
pub mod search_chunks;

#[cfg(test)]
mod analyze_operation_test;
#[cfg(test)]
mod create_operation_test;
#[cfg(test)]
mod get_score_history_test;

