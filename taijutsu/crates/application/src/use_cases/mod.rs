//! Use Cases — Points d'entrée de la logique métier.

pub mod analyze_operation;
pub mod create_operation;
pub mod get_operation;
pub mod get_operation_diff;
pub mod list_operations;
pub mod search_chunks;

#[cfg(test)]
mod analyze_operation_test;
#[cfg(test)]
mod create_operation_test;
