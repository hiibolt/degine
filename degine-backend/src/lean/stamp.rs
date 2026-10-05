use super::codegen::sanitize;
use super::DepGraph;
use crate::model::{Fact, Rule};

/// Inputs that decide whether a saved proof is still the right one.
///
/// A proved graph is stamped from the facts and rules it actually used.
/// An invalid result is stamped from the whole library, because a new
/// theorem can make a failed proof succeed.
pub fn library_stamp(facts: &[Fact], rules: &[Rule], target_line: &str) -> String {
    let mut lines: Vec<String> = facts.iter().map(fact_line).chain(rules.iter().map(rule_line)).collect();
    lines.push(target_line.to_string());
    lines.sort();
    lines.dedup();
    lines.join("\n")
}

pub fn proved_stamp(
    graph: &DepGraph,
    facts: &[Fact],
    rules: &[Rule],
    target_id: &str,
    target_line: &str,
) -> Option<String> {
    let mut lines = vec![target_line.to_string()];
    for node in &graph.nodes {
        lines.push(node_line(&node.name, facts, rules, target_id, target_line)?);
    }
    lines.sort();
    lines.dedup();
    Some(lines.join("\n"))
}

/// Lean rejected the claim itself. A toolchain crash is not stable.
pub fn stable_invalid(diagnostics: &str) -> bool {
    diagnostics.contains("error:")
}

pub fn fact_line(fact: &Fact) -> String {
    format!(
        "fact\t{}\t{}\t{}",
        fact.id,
        fact.role,
        fact.formula.as_deref().unwrap_or("")
    )
}

pub fn rule_line(rule: &Rule) -> String {
    let mut premises = rule.premises.clone();
    premises.sort();
    format!("rule\t{}\t{}\t{}", rule.id, premises.join(","), rule.conclusion)
}

pub fn assert_line(id: &str, formula: &str, assumes: &[String]) -> String {
    let mut assumes = assumes.to_vec();
    assumes.sort();
    format!("assert\t{id}\t{}\t{formula}", assumes.join(","))
}

fn node_line(
    name: &str,
    facts: &[Fact],
    rules: &[Rule],
    target_id: &str,
    target_line: &str,
) -> Option<String> {
    if let Some(suffix) = name.strip_prefix("Debate.fact_") {
        let fact = facts.iter().find(|fact| sanitize(&fact.id).ok().as_deref() == Some(suffix))?;
        return Some(fact_line(fact));
    }
    if let Some(suffix) = name.strip_prefix("Debate.rule_") {
        if sanitize(target_id).ok().as_deref() == Some(suffix) {
            return Some(target_line.to_string());
        }
        let rule = rules.iter().find(|rule| sanitize(&rule.id).ok().as_deref() == Some(suffix))?;
        return Some(rule_line(rule));
    }
    None
}
