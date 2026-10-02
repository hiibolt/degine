use std::collections::HashSet;

use anyhow::Result;

use crate::db::Db;
use crate::lean::{formula_atoms, formula_then};
use crate::model::Fact;

pub struct Grant {
    pub assert_ids: HashSet<String>,
    pub fact_ids: HashSet<String>,
    pub atoms: HashSet<String>,
}

impl Grant {
    pub fn sees_target(&self, target_type: &str, target_id: &str) -> bool {
        match target_type {
            "assert" => self.assert_ids.contains(target_id),
            "fact" => self.fact_ids.contains(target_id) || self.atoms.contains(target_id),
            _ => false,
        }
    }
}

pub fn grant_for(db: &Db, username: &str) -> Result<Grant> {
    let facts = db.list_facts()?;
    let shared = db.shares_for(username)?;
    let asserts = db.list_asserts()?;
    let mut atoms = HashSet::new();
    let mut assert_ids = HashSet::new();
    for assert in &asserts {
        if !shared.iter().any(|id| id == &assert.id) {
            continue;
        }
        assert_ids.insert(assert.id.clone());
        for id in formula_atoms(&assert.formula) {
            atoms.insert(id);
        }
    }
    let fact_ids = closure(&facts, &mut atoms);
    Ok(Grant {
        assert_ids,
        fact_ids,
        atoms,
    })
}

fn closure(facts: &[Fact], atoms: &mut HashSet<String>) -> HashSet<String> {
    let mut included = HashSet::new();
    loop {
        let mut grew = false;
        for fact in facts {
            if included.contains(&fact.id) {
                continue;
            }
            if fact.role != "theorem" {
                if atoms.contains(&fact.id) {
                    included.insert(fact.id.clone());
                    grew = true;
                }
                continue;
            }
            let Some(formula) = fact.formula.as_deref() else {
                continue;
            };
            let Some(then_id) = formula_then(formula) else {
                continue;
            };
            if !atoms.contains(&then_id) {
                continue;
            }
            included.insert(fact.id.clone());
            for id in formula_atoms(formula) {
                atoms.insert(id);
            }
            grew = true;
        }
        if !grew {
            break;
        }
    }
    included
}
