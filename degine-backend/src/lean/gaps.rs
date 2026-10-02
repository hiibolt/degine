//! Which personal facts are off, and which of them would make a goal follow.

use std::collections::HashSet;

use super::formula::{self, Formula};

/// Smallest set of `optional` atoms which, added to `given`, lets `goal` follow
/// from implication theorems. `None` means no subset does.
pub fn missing<'a>(
    theorems: impl IntoIterator<Item = &'a str>,
    given: &HashSet<String>,
    optional: &[String],
    goal: &str,
) -> Option<Vec<String>> {
    let rules = horn(theorems);
    let mentioned: Vec<String> = optional
        .iter()
        .filter(|id| rules.iter().any(|(ante, _)| ante.contains(*id)) || *id == goal)
        .cloned()
        .collect();
    let pool = if mentioned.len() <= 18 {
        mentioned
    } else {
        mentioned[..18].to_vec()
    };
    for size in 0..=pool.len() {
        let mut pick = Vec::new();
        if search(size, 0, &pool, &mut pick, given, &rules, goal) {
            return Some(pick);
        }
    }
    None
}

fn horn<'a>(theorems: impl IntoIterator<Item = &'a str>) -> Vec<(HashSet<String>, String)> {
    let mut rules = Vec::new();
    for text in theorems {
        let Ok(formula) = formula::parse(text) else {
            continue;
        };
        let Formula::Imp(ante, cons) = formula else {
            continue;
        };
        let Formula::Atom(id) = *cons else {
            continue;
        };
        let mut need = HashSet::new();
        if !ante_atoms(&ante, &mut need) {
            continue;
        }
        rules.push((need, id));
    }
    rules
}

/// `false` when the antecedent is not a plain and/or/atom (an `or` would need
/// either branch, which this hint does not search).
fn ante_atoms(formula: &Formula, out: &mut HashSet<String>) -> bool {
    match formula {
        Formula::Atom(id) => {
            out.insert(id.clone());
            true
        }
        Formula::And(parts) => parts.iter().all(|part| ante_atoms(part, out)),
        Formula::True => true,
        _ => false,
    }
}

fn reaches(
    given: &HashSet<String>,
    extra: &[String],
    rules: &[(HashSet<String>, String)],
    goal: &str,
) -> bool {
    let mut have = given.clone();
    have.extend(extra.iter().cloned());
    loop {
        let mut grew = false;
        for (need, conclusion) in rules {
            if have.contains(conclusion) {
                continue;
            }
            if need.iter().all(|id| have.contains(id)) {
                have.insert(conclusion.clone());
                grew = true;
            }
        }
        if !grew {
            break;
        }
    }
    have.contains(goal)
}

fn search(
    left: usize,
    start: usize,
    pool: &[String],
    pick: &mut Vec<String>,
    given: &HashSet<String>,
    rules: &[(HashSet<String>, String)],
    goal: &str,
) -> bool {
    if left == 0 {
        return reaches(given, pick, rules, goal);
    }
    for index in start..pool.len() {
        if pool.len() - index < left {
            break;
        }
        pick.push(pool[index].clone());
        if search(left - 1, index + 1, pool, pick, given, rules, goal) {
            return true;
        }
        pick.pop();
    }
    false
}
