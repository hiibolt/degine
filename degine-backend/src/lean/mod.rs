//! Codegen, proof search, and the Lean helper bridge.
//!
//! Facts become axioms. Rules become theorems. A single worker writes the
//! `.lean` file, builds it, and asks the extractor for the dependency graph.

mod bridge;
mod codegen;
mod formula;
mod gaps;
mod prove;
mod queue;

pub use gaps::missing;

pub use bridge::{DepEdge, DepGraph, DepNode, LeanOutcome};
pub use codegen::{fact_decl_name, rule_decl_name};
pub use formula::{formula_atoms, formula_then};
pub use queue::{JobRequest, LeanEvent, LeanQueue};
