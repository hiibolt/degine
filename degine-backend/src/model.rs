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
    /// Criterion ids treated as given for this assert only.
    #[serde(default)]
    pub assumes: Vec<String>,
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Person {
    pub id: String,
    pub name: String,
    pub on: Vec<String>,
    /// `{other}` facts that hold. Each entry is one other person.
    #[serde(default)]
    pub links: Vec<PersonLink>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PersonLink {
    pub fact: String,
    pub other: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PersonalFact {
    pub id: String,
    pub claim: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct Workspace {
    pub id: String,
    pub name: String,
    pub role: String,
    pub creator: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct Member {
    pub user_id: String,
    pub username: Option<String>,
    pub role: String,
    pub creator: bool,
}

/// One snapshot of a debate, checked as a single Lean job.
#[derive(Debug, Clone)]
pub struct JobRequest {
    pub workspace_id: String,
    pub facts: Vec<Fact>,
    pub rules: Vec<Rule>,
    pub target_rule_id: String,
    /// A person-scoped check. Its result is not saved over the assert graph.
    pub ephemeral: bool,
    /// Canonical line for this target, shared by the skip check and the saved stamp.
    pub target_line: String,
    /// Whole-library stamp. Saved when Lean rejects the claim.
    pub stamp: String,
}
