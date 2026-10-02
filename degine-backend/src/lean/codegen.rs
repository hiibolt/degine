use std::collections::{BTreeMap, BTreeSet, HashMap};

use anyhow::{bail, Context, Result};

use crate::model::{JobRequest, Rule};

use super::formula::{self, Formula};
use super::prove::{self, lean_type};

pub(crate) struct Module {
    pub source: String,
    pub target_decl: String,
}

pub(crate) enum Prepared {
    Ready(Module),
    Rejected { diagnostics: String },
}

pub fn fact_decl_name(id: &str) -> Result<String> {
    Ok(format!("Debate.fact_{}", sanitize(id)?))
}

pub fn rule_decl_name(id: &str) -> Result<String> {
    Ok(format!("Debate.rule_{}", sanitize(id)?))
}

pub(crate) fn prepare(job: &JobRequest) -> Result<Prepared> {
    match prepare_inner(job) {
        Ok(prepared) => Ok(prepared),
        Err(rejection) => match rejection {
            PrepError::Reject(diagnostics) => Ok(Prepared::Rejected { diagnostics }),
            PrepError::Bug(err) => Err(err),
        },
    }
}

enum PrepError {
    Reject(String),
    Bug(anyhow::Error),
}

impl From<anyhow::Error> for PrepError {
    fn from(err: anyhow::Error) -> Self {
        PrepError::Bug(err)
    }
}

fn reject(message: impl Into<String>) -> PrepError {
    PrepError::Reject(message.into())
}

enum Standing {
    Given,
    Criterion,
    Warrant,
}

struct ResolvedFact {
    standing: Standing,
    formula: Formula,
    claim: String,
    citations: Vec<String>,
}

fn prepare_inner(job: &JobRequest) -> std::result::Result<Prepared, PrepError> {
    let mut facts: BTreeMap<String, ResolvedFact> = BTreeMap::new();
    for fact in &job.facts {
        if fact.id.is_empty() {
            return Err(reject("a fact is missing an id"));
        }
        if facts.contains_key(&fact.id) {
            return Err(reject(format!("duplicate fact id `{}`", fact.id)));
        }
        let (standing, formula) = match fact.role.as_str() {
            "criterion" if fact.formula.is_none() => {
                (Standing::Criterion, Formula::Atom(fact.id.clone()))
            }
            "fact" | "theorem" | "" | "criterion" => match &fact.formula {
                None if fact.role == "theorem" => {
                    return Err(reject(format!(
                        "theorem `{}` is missing a formula",
                        fact.id
                    )));
                }
                None => (Standing::Given, Formula::Atom(fact.id.clone())),
                Some(_) if fact.role == "criterion" => {
                    return Err(reject(format!(
                        "criterion `{}` cannot carry a formula",
                        fact.id
                    )));
                }
                Some(text) => {
                    let formula = formula::parse(text).map_err(|err| {
                        reject(format!(
                            "fact `{}` has an invalid formula: {err:#}",
                            fact.id
                        ))
                    })?;
                    (Standing::Warrant, formula)
                }
            },
            other => {
                return Err(reject(format!(
                    "fact `{}` has an unknown role `{other}`",
                    fact.id
                )));
            }
        };
        facts.insert(
            fact.id.clone(),
            ResolvedFact {
                standing,
                formula,
                claim: fact.claim.clone(),
                citations: fact.citations.clone(),
            },
        );
    }

    let mut rules: BTreeMap<String, &Rule> = BTreeMap::new();
    for rule in &job.rules {
        if rule.id.is_empty() {
            return Err(reject("a rule is missing an id"));
        }
        if facts.contains_key(&rule.id) || rules.contains_key(&rule.id) {
            return Err(reject(format!("duplicate id `{}`", rule.id)));
        }
        rules.insert(rule.id.clone(), rule);
    }
    if !rules.contains_key(&job.target_rule_id) {
        return Err(reject(format!(
            "unknown target rule `{}`",
            job.target_rule_id
        )));
    }

    let ordered = order_rules(&job.target_rule_id, &facts, &rules)?;
    let mut conclusions: HashMap<String, Formula> = HashMap::new();
    for rule in &ordered {
        let conclusion = formula::parse(&rule.conclusion).map_err(|err| {
            reject(format!(
                "rule `{}` has an invalid conclusion: {err:#}",
                rule.id
            ))
        })?;
        for premise in &rule.premises {
            if let Some(fact) = facts.get(premise) {
                if matches!(fact.standing, Standing::Criterion) {
                    return Err(reject(format!(
                        "rule `{}` lists criterion `{premise}` as given. Assume it in the formula instead",
                        rule.id
                    )));
                }
            } else if !rules.contains_key(premise) {
                return Err(reject(format!(
                    "rule `{}` cites unknown premise `{}`",
                    rule.id, premise
                )));
            }
        }
        conclusions.insert(rule.id.clone(), conclusion);
    }

    let mut atom_ids: Vec<String> = Vec::new();
    for (id, fact) in &facts {
        match fact.standing {
            Standing::Given | Standing::Criterion => {
                formula::atoms(&Formula::Atom(id.clone()), &mut atom_ids);
            }
            Standing::Warrant => formula::atoms(&fact.formula, &mut atom_ids),
        }
    }
    for conclusion in conclusions.values() {
        formula::atoms(conclusion, &mut atom_ids);
    }
    atom_ids.sort();
    atom_ids.dedup();
    for atom in &atom_ids {
        if rules.contains_key(atom) {
            return Err(reject(format!(
                "`fact:{atom}` names rule `{atom}`, not a proposition"
            )));
        }
        if let Some(fact) = facts.get(atom) {
            if matches!(fact.standing, Standing::Warrant) {
                return Err(reject(format!(
                    "`fact:{atom}` is a warrant, not an atomic proposition"
                )));
            }
        }
    }

    let mut suffixes: HashMap<String, String> = HashMap::new();
    let mut taken: HashMap<String, String> = HashMap::new();
    let mut named: Vec<String> = facts.keys().cloned().chain(rules.keys().cloned()).collect();
    named.extend(atom_ids.iter().cloned());
    named.sort();
    named.dedup();
    for id in &named {
        let suffix = sanitize(id).map_err(|err| reject(format!("{err:#}")))?;
        if let Some(other) = taken.insert(suffix.clone(), id.clone()) {
            return Err(reject(format!(
                "ids `{other}` and `{id}` collapse to the same Lean name `{suffix}`"
            )));
        }
        suffixes.insert(id.clone(), suffix);
    }

    let mut atom_types: HashMap<String, String> = HashMap::new();
    for id in &atom_ids {
        atom_types.insert(id.clone(), format!("atom_{}", suf(&suffixes, id)?));
    }

    let mut source = String::from(
        "-- Generated by degine. Do not edit.\n\
         set_option autoImplicit false\n\
         set_option relaxedAutoImplicit false\n\n\
         namespace Debate\n\n",
    );
    for id in &atom_ids {
        source.push_str(&format!("axiom {} : Prop\n\n", atom_types[id]));
    }
    for (id, fact) in &facts {
        if matches!(fact.standing, Standing::Criterion) {
            source.push_str(&format!(
                "-- criterion {id}: {}\n\n",
                one_line(&fact.claim)
            ));
            continue;
        }
        source.push_str(&fact_comment(id, fact));
        let decl = format!("fact_{}", suf(&suffixes, id)?);
        let ty = lean_type(&fact.formula, &atom_types);
        source.push_str(&format!("axiom {decl} : {ty}\n\n"));
    }
    for rule in &ordered {
        source.push_str(&rule_block(
            rule,
            &facts,
            &conclusions,
            &suffixes,
            &atom_types,
        )?);
    }
    source.push_str("end Debate\n");

    let target_suffix = suf(&suffixes, &job.target_rule_id)?;
    Ok(Prepared::Ready(Module {
        source,
        target_decl: format!("Debate.rule_{target_suffix}"),
    }))
}

fn fact_comment(id: &str, fact: &ResolvedFact) -> String {
    let mut line = format!("-- fact {id}: {}", one_line(&fact.claim));
    let citations = fact
        .citations
        .iter()
        .filter(|citation| !citation.is_empty())
        .map(|citation| one_line(citation))
        .collect::<Vec<_>>()
        .join("; ");
    if !citations.is_empty() {
        line.push_str(&format!(" ({citations})"));
    }
    line.push('\n');
    line
}

fn rule_block(
    rule: &Rule,
    facts: &BTreeMap<String, ResolvedFact>,
    conclusions: &HashMap<String, Formula>,
    suffixes: &HashMap<String, String>,
    atom_types: &HashMap<String, String>,
) -> std::result::Result<String, PrepError> {
    let conclusion = &conclusions[&rule.id];
    let mut premises = Vec::new();
    for premise in &rule.premises {
        if let Some(fact) = facts.get(premise) {
            premises.push((
                format!("fact_{}", suf(suffixes, premise)?),
                fact.formula.clone(),
            ));
        } else if conclusions.contains_key(premise) {
            premises.push((
                format!("rule_{}", suf(suffixes, premise)?),
                conclusions[premise].clone(),
            ));
        } else {
            return Err(reject(format!(
                "rule `{}` cites unknown premise `{premise}`",
                rule.id
            )));
        }
    }
    let ty = lean_type(conclusion, atom_types);
    let decl = format!("rule_{}", suf(suffixes, &rule.id)?);
    let body = match prove::prove(conclusion, &premises, atom_types) {
        Some(term) => format!("by\n  exact {term}"),
        None => format!(
            "by\n  fail \"rule {}: conclusion does not follow from the premises\"",
            one_line(&rule.id)
        ),
    };
    Ok(format!(
        "-- rule {}: {}\n-- premises: {}\ntheorem {decl} : {ty} := {body}\n\n",
        rule.id,
        conclusion,
        if rule.premises.is_empty() {
            "(none)".to_string()
        } else {
            rule.premises.join(", ")
        }
    ))
}

fn order_rules<'a>(
    target: &str,
    facts: &BTreeMap<String, ResolvedFact>,
    rules: &BTreeMap<String, &'a Rule>,
) -> std::result::Result<Vec<&'a Rule>, PrepError> {
    let mut visiting = BTreeSet::new();
    let mut visited = BTreeSet::new();
    let mut ordered = Vec::new();
    fn walk<'a>(
        id: &str,
        facts: &BTreeMap<String, ResolvedFact>,
        rules: &BTreeMap<String, &'a Rule>,
        visiting: &mut BTreeSet<String>,
        visited: &mut BTreeSet<String>,
        ordered: &mut Vec<&'a Rule>,
    ) -> std::result::Result<(), PrepError> {
        if visited.contains(id) {
            return Ok(());
        }
        if !visiting.insert(id.to_string()) {
            return Err(reject(format!("rule `{id}` is part of a premise cycle")));
        }
        let rule = rules
            .get(id)
            .copied()
            .ok_or_else(|| reject(format!("unknown rule `{id}`")))?;
        for premise in &rule.premises {
            if facts.contains_key(premise) {
                continue;
            }
            if rules.contains_key(premise) {
                walk(premise, facts, rules, visiting, visited, ordered)?;
            }
        }
        visiting.remove(id);
        visited.insert(id.to_string());
        ordered.push(rule);
        Ok(())
    }
    walk(
        target,
        facts,
        rules,
        &mut visiting,
        &mut visited,
        &mut ordered,
    )?;
    Ok(ordered)
}

fn suf<'a>(
    suffixes: &'a HashMap<String, String>,
    id: &str,
) -> std::result::Result<&'a str, PrepError> {
    suffixes
        .get(id)
        .map(String::as_str)
        .ok_or_else(|| PrepError::Bug(anyhow::anyhow!("missing Lean name for `{id}`")))
}

fn one_line(text: &str) -> String {
    text.replace(['\n', '\r', '"'], " ")
}

pub fn sanitize(id: &str) -> Result<String> {
    if id.is_empty() {
        bail!("empty id");
    }
    let mut out = String::new();
    for ch in id.chars() {
        if ch.is_ascii_alphanumeric() || ch == '_' {
            out.push(ch);
        } else {
            out.push('_');
        }
    }
    if out
        .chars()
        .next()
        .context("sanitized id was empty")?
        .is_ascii_digit()
    {
        out.insert(0, 'x');
    }
    Ok(out)
}
