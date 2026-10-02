//! Goal-directed constructive proof search.
//!
//! The term we emit is checked again by Lean. If search fails, codegen emits
//! a `fail` tactic so the invalid chain shows up as a compile diagnostic.

use std::collections::{HashMap, HashSet};

use super::formula::{self, Formula};

#[derive(Clone)]
struct Hyp {
    uid: u32,
    term: String,
    formula: Formula,
}

struct Search<'a> {
    hyps: &'a [Hyp],
    banned: &'a HashSet<u32>,
    atoms: &'a HashMap<String, String>,
    fuel: u32,
    binders: &'a mut u32,
    /// When false, do not try to prove an arbitrary goal from `False`.
    exfalso: bool,
}

pub(crate) fn prove(
    goal: &Formula,
    premises: &[(String, Formula)],
    atoms: &HashMap<String, String>,
) -> Option<String> {
    let goal = formula::normalize(goal);
    let mut hyps = Vec::new();
    let mut next_uid = 0;
    for (term, formula) in premises {
        next_uid = saturate(
            &mut hyps,
            next_uid,
            term.clone(),
            formula::normalize(formula),
        );
    }
    let mut binders = 0;
    let banned = HashSet::new();
    Search {
        hyps: &hyps,
        banned: &banned,
        atoms,
        fuel: 48,
        binders: &mut binders,
        exfalso: true,
    }
    .prove(&goal)
}

fn saturate(hyps: &mut Vec<Hyp>, mut next_uid: u32, term: String, formula: Formula) -> u32 {
    let mut stack = vec![(term, formula)];
    while let Some((term, formula)) = stack.pop() {
        if hyps.iter().any(|hyp| hyp.formula == formula) {
            continue;
        }
        let uid = next_uid;
        next_uid += 1;
        if let Formula::And(parts) = formula.clone() {
            let base = term.clone();
            let part_count = parts.len();
            hyps.push(Hyp { uid, term, formula });
            for (index, part) in parts.into_iter().enumerate() {
                stack.push((and_proj(&base, part_count, index), part));
            }
        } else {
            hyps.push(Hyp { uid, term, formula });
        }
    }
    next_uid
}

impl Search<'_> {
    fn prove(&mut self, goal: &Formula) -> Option<String> {
        if self.fuel == 0 {
            return None;
        }
        self.fuel -= 1;
        let fuel_after = self.fuel;

        if let Some(term) = self.exact(goal) {
            return Some(term);
        }
        if matches!(goal, Formula::True) {
            return Some("True.intro".to_string());
        }
        if let Some(term) = self.false_elim(goal) {
            return Some(term);
        }
        if let Formula::And(parts) = goal {
            let mut terms = Vec::with_capacity(parts.len());
            for part in parts {
                self.fuel = fuel_after;
                terms.push(self.prove(part)?);
            }
            return Some(and_intro(&terms));
        }
        if let Formula::Imp(ante, cons) = goal {
            return self.intro(ante, cons);
        }
        if let Formula::Not(inner) = goal {
            return self.intro(inner, &Formula::False);
        }
        if matches!(goal, Formula::False) {
            if let Some(term) = self.not_elim() {
                return Some(term);
            }
        }
        if let Formula::Or(parts) = goal {
            if let Some(term) = self.prove_or(parts, goal, fuel_after) {
                return Some(term);
            }
        }
        if let Some(term) = self.imp_elim(goal, fuel_after) {
            return Some(term);
        }
        if let Some(term) = self.or_elim(goal, fuel_after) {
            return Some(term);
        }
        if self.exfalso && !matches!(goal, Formula::False) {
            self.fuel = fuel_after;
            self.exfalso = false;
            if let Some(bot) = self.prove(&Formula::False) {
                return Some(format!(
                    "(False.elim {bot} : {})",
                    lean_type(goal, self.atoms)
                ));
            }
        }
        None
    }

    fn exact(&self, goal: &Formula) -> Option<String> {
        self.hyps.iter().find_map(|hyp| {
            (hyp.formula == *goal && !self.banned.contains(&hyp.uid)).then(|| hyp.term.clone())
        })
    }

    fn false_elim(&self, goal: &Formula) -> Option<String> {
        let hyp = self
            .hyps
            .iter()
            .find(|hyp| matches!(hyp.formula, Formula::False) && !self.banned.contains(&hyp.uid))?;
        Some(format!(
            "(False.elim {} : {})",
            hyp.term,
            lean_type(goal, self.atoms)
        ))
    }

    fn intro(&mut self, assumed: &Formula, body_goal: &Formula) -> Option<String> {
        let name = self.fresh();
        let ty = lean_type(assumed, self.atoms);
        let mut hyps = self.hyps.to_vec();
        saturate(&mut hyps, self.next_uid(), name.clone(), assumed.clone());
        let banned = self.banned.clone();
        let proved = {
            let mut inner = Search {
                hyps: &hyps,
                banned: &banned,
                atoms: self.atoms,
                fuel: self.fuel,
                binders: self.binders,
                exfalso: self.exfalso,
            };
            inner.prove(body_goal).map(|body| (body, inner.fuel))
        };
        let (body, fuel) = proved?;
        self.fuel = fuel;
        Some(format!("(fun {name} : {ty} => {body})"))
    }

    fn not_elim(&mut self) -> Option<String> {
        let candidates: Vec<(u32, String, Formula)> = self
            .hyps
            .iter()
            .filter(|hyp| !self.banned.contains(&hyp.uid))
            .filter_map(|hyp| match &hyp.formula {
                Formula::Not(inner) => Some((hyp.uid, hyp.term.clone(), inner.as_ref().clone())),
                _ => None,
            })
            .collect();
        let fuel = self.fuel;
        for (uid, term, inner) in candidates {
            self.fuel = fuel;
            let mut banned = self.banned.clone();
            banned.insert(uid);
            let proved = {
                let mut search = Search {
                    hyps: self.hyps,
                    banned: &banned,
                    atoms: self.atoms,
                    fuel: self.fuel,
                    binders: self.binders,
                    exfalso: self.exfalso,
                };
                search.prove(&inner).map(|proof| (proof, search.fuel))
            };
            if let Some((proof, next_fuel)) = proved {
                self.fuel = next_fuel;
                return Some(format!("({term} {proof})"));
            }
        }
        None
    }

    fn imp_elim(&mut self, goal: &Formula, fuel: u32) -> Option<String> {
        let candidates: Vec<(u32, String, Formula)> = self
            .hyps
            .iter()
            .filter(|hyp| !self.banned.contains(&hyp.uid))
            .filter_map(|hyp| match &hyp.formula {
                Formula::Imp(ante, cons) if cons.as_ref() == goal => {
                    Some((hyp.uid, hyp.term.clone(), ante.as_ref().clone()))
                }
                _ => None,
            })
            .collect();
        for (uid, term, ante) in candidates {
            self.fuel = fuel;
            let mut banned = self.banned.clone();
            banned.insert(uid);
            let proved = {
                let mut search = Search {
                    hyps: self.hyps,
                    banned: &banned,
                    atoms: self.atoms,
                    fuel: self.fuel,
                    binders: self.binders,
                    exfalso: self.exfalso,
                };
                search.prove(&ante).map(|proof| (proof, search.fuel))
            };
            if let Some((proof, next_fuel)) = proved {
                self.fuel = next_fuel;
                return Some(format!("({term} {proof})"));
            }
        }
        None
    }

    fn or_elim(&mut self, goal: &Formula, fuel: u32) -> Option<String> {
        let candidates: Vec<(u32, String, Vec<Formula>)> = self
            .hyps
            .iter()
            .filter(|hyp| !self.banned.contains(&hyp.uid))
            .filter_map(|hyp| match &hyp.formula {
                Formula::Or(parts) if parts.len() >= 2 => {
                    Some((hyp.uid, hyp.term.clone(), parts.clone()))
                }
                _ => None,
            })
            .collect();
        for (uid, term, parts) in candidates {
            self.fuel = fuel;
            let kept: Vec<Hyp> = self
                .hyps
                .iter()
                .filter(|hyp| hyp.uid != uid)
                .cloned()
                .collect();
            let mut banned = self.banned.clone();
            banned.insert(uid);
            let left = parts[0].clone();
            let right = if parts.len() == 2 {
                parts[1].clone()
            } else {
                Formula::Or(parts[1..].to_vec())
            };
            let Some(left_proof) = self.branch(&kept, &banned, &left, goal) else {
                continue;
            };
            self.fuel = fuel;
            let Some(right_proof) = self.branch(&kept, &banned, &right, goal) else {
                continue;
            };
            return Some(format!(
                "(Or.elim {term} (fun {} : {} => {}) (fun {} : {} => {}))",
                left_proof.0,
                lean_type(&left, self.atoms),
                left_proof.1,
                right_proof.0,
                lean_type(&right, self.atoms),
                right_proof.1
            ));
        }
        None
    }

    fn prove_or(&mut self, parts: &[Formula], _goal: &Formula, fuel: u32) -> Option<String> {
        for index in 0..parts.len() {
            self.fuel = fuel;
            let Some(term) = self.prove(&parts[index]) else {
                continue;
            };
            return Some(inject_or(index, parts, &term, self.atoms));
        }
        None
    }

    fn branch(
        &mut self,
        kept: &[Hyp],
        banned: &HashSet<u32>,
        assumed: &Formula,
        goal: &Formula,
    ) -> Option<(String, String)> {
        let mut hyps = kept.to_vec();
        let name = self.fresh();
        let uid = self.next_uid_from(&hyps);
        saturate(&mut hyps, uid, name.clone(), assumed.clone());
        let proved = {
            let mut search = Search {
                hyps: &hyps,
                banned,
                atoms: self.atoms,
                fuel: self.fuel,
                binders: self.binders,
                exfalso: self.exfalso,
            };
            let proof = search.prove(goal);
            proof.map(|proof| (proof, search.fuel))
        };
        let (proof, fuel) = proved?;
        self.fuel = fuel;
        Some((name, proof))
    }

    fn next_uid_from(&self, hyps: &[Hyp]) -> u32 {
        hyps.iter()
            .map(|hyp| hyp.uid)
            .max()
            .map_or(0, |uid| uid + 1)
    }

    fn fresh(&mut self) -> String {
        let name = format!("h{}", *self.binders);
        *self.binders += 1;
        name
    }

    fn next_uid(&self) -> u32 {
        self.hyps
            .iter()
            .map(|hyp| hyp.uid)
            .max()
            .map_or(0, |uid| uid + 1)
    }
}

fn and_intro(terms: &[String]) -> String {
    match terms {
        [only] => only.clone(),
        [head, rest @ ..] => format!("(And.intro {head} {})", and_intro(rest)),
        [] => "True.intro".to_string(),
    }
}

fn and_proj(term: &str, len: usize, index: usize) -> String {
    let mut current = term.to_string();
    for _ in 0..index {
        current = format!("(And.right {current})");
    }
    if index + 1 == len {
        current
    } else {
        format!("(And.left {current})")
    }
}

fn or_type(parts: &[Formula], atoms: &HashMap<String, String>) -> String {
    match parts {
        [only] => lean_type(only, atoms),
        [head, rest @ ..] => format!("({} \\/ {})", lean_type(head, atoms), or_type(rest, atoms)),
        [] => "False".to_string(),
    }
}

fn inject_or(index: usize, parts: &[Formula], term: &str, atoms: &HashMap<String, String>) -> String {
    if parts.len() <= 1 {
        return term.to_string();
    }
    let full = or_type(parts, atoms);
    if index == 0 {
        return format!("(Or.inl {term} : {full})");
    }
    let inner = inject_or(index - 1, &parts[1..], term, atoms);
    format!("(Or.inr {inner} : {full})")
}

pub(crate) fn lean_type(formula: &Formula, atoms: &HashMap<String, String>) -> String {
    lean_type_normalized(&formula::normalize(formula), atoms)
}

fn lean_type_normalized(formula: &Formula, atoms: &HashMap<String, String>) -> String {
    match formula {
        Formula::Atom(id) => atoms
            .get(id)
            .cloned()
            .unwrap_or_else(|| format!("atom_MISSING_{id}")),
        Formula::And(parts) => nest_type(parts, "/\\", atoms),
        Formula::Or(parts) => or_type(parts, atoms),
        Formula::Imp(left, right) => format!(
            "({} -> {})",
            lean_type(left, atoms),
            lean_type(right, atoms)
        ),
        Formula::Not(inner) => format!("(Not {})", lean_type(inner, atoms)),
        Formula::True => "True".to_string(),
        Formula::False => "False".to_string(),
    }
}

fn nest_type(parts: &[Formula], op: &str, atoms: &HashMap<String, String>) -> String {
    match parts {
        [only] => lean_type(only, atoms),
        [head, rest @ ..] => format!(
            "({} {op} {})",
            lean_type(head, atoms),
            nest_type(rest, op, atoms)
        ),
        [] => "True".to_string(),
    }
}
