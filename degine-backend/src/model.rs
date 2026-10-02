//! Facts are cited assumptions. Rules are conclusions Lean has to prove.

use serde::{Deserialize, Serialize};

/// A cited assumption.
///
/// `citations` are the messages, articles, and other sources this claim
/// rests on. One fact can carry several.
///
/// `role` is `fact`, `criterion`, or `theorem`.
///
/// A fact is an atomic proposition named `fact:<id>`, which Lean treats as
/// true. A criterion is the same kind of atom, but it is not treated as true.
/// It is only assumed while an assert's formula puts it on the if-side.
/// A theorem is a cited warrant (for example `imp(fact:signed, fact:binding)`)
/// and does not by itself introduce that atom.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Fact {
    pub id: String,
    pub claim: String,
    pub citations: Vec<String>,
    pub formula: Option<String>,
    /// `fact`, `criterion`, or `theorem`.
    pub role: String,
}

/// A claim the library is asked to prove. The formula is the whole claim.
/// Facts and rules stay in the library. An assert does not list premises.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Assert {
    pub id: String,
    pub title: String,
    pub description: String,
    pub formula: String,
}

/// A derived step. `conclusion` is a formula, not prose.
///
/// The formula language is documented on [`crate::lean::formula`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Rule {
    pub id: String,
    pub premises: Vec<String>,
    pub conclusion: String,
}

/// A note on a fact, a rule, or a conclusion (a rule id).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Comment {
    pub id: i64,
    pub target_type: String,
    pub target_id: String,
    pub author: String,
    pub body: String,
    pub created_at: String,
    pub resolved: bool,
}

/// One snapshot of a debate, checked as a single Lean job.
#[derive(Debug, Clone)]
pub struct JobRequest {
    pub facts: Vec<Fact>,
    pub rules: Vec<Rule>,
    pub target_rule_id: String,
}
