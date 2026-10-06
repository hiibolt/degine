use std::collections::HashSet;

use axum::extract::{Path, State};
use axum::Json;
use serde::{Deserialize, Serialize};
use axum::http::StatusCode;

use crate::lean::{missing, DepGraph, LeanOutcome};
use crate::model::{Fact, Person, PersonalFact};

use crate::access::{Editor, Scope};

use super::{assert_job, clean_text, require_id, AppError, AppState, IdPath};

#[derive(Deserialize)]
pub(super) struct PersonWrite {
    id: String,
    name: String,
}

#[derive(Deserialize)]
pub(super) struct PersonalWrite {
    id: String,
    claim: String,
}

#[derive(Deserialize)]
pub(super) struct ToggleWrite {
    on: bool,
}

#[derive(Serialize)]
pub(super) struct PersonCheck {
    status: String,
    graph: Option<DepGraph>,
    diagnostics: Option<String>,
    missing: Vec<PersonalFact>,
    facts: Vec<Fact>,
}

pub(super) async fn list_people(
    State(state): State<AppState>,
    scope: Scope,
) -> Result<Json<Vec<Person>>, AppError> {
    Ok(Json(state.db.list_people(&scope)?))
}

pub(super) async fn create_person(
    State(state): State<AppState>,
    editor: Editor,
    Json(body): Json<PersonWrite>,
) -> Result<Json<Person>, AppError> {
    require_id(&body.id)?;
    let name = clean_text(&body.name, "name")?;
    state.db.insert_person(&editor, &body.id, &name)?;
    Ok(Json(Person {
        id: body.id,
        name,
        on: Vec::new(),
        links: Vec::new(),
    }))
}

pub(super) async fn delete_person(
    State(state): State<AppState>,
    editor: Editor,
    Path(IdPath { id }): Path<IdPath>,
) -> Result<StatusCode, AppError> {
    if !state.db.delete_person(&editor, &id)? {
        return Err(AppError::not_found());
    }
    Ok(StatusCode::NO_CONTENT)
}

pub(super) async fn list_personal(
    State(state): State<AppState>,
    scope: Scope,
) -> Result<Json<Vec<PersonalFact>>, AppError> {
    Ok(Json(state.db.list_personal_facts(&scope)?))
}

pub(super) async fn create_personal(
    State(state): State<AppState>,
    editor: Editor,
    Json(body): Json<PersonalWrite>,
) -> Result<Json<PersonalFact>, AppError> {
    require_id(&body.id)?;
    let claim = clean_text(&body.claim, "claim")?;
    state.db.insert_personal_fact(&editor, &body.id, &claim)?;
    Ok(Json(PersonalFact { id: body.id, claim }))
}

pub(super) async fn delete_personal(
    State(state): State<AppState>,
    editor: Editor,
    Path(IdPath { id }): Path<IdPath>,
) -> Result<StatusCode, AppError> {
    if !state.db.delete_personal_fact(&editor, &id)? {
        return Err(AppError::not_found());
    }
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Deserialize)]
pub(super) struct TogglePath {
    person: String,
    fact: String,
}

#[derive(Deserialize)]
pub(super) struct CheckPath {
    id: String,
    person: String,
}

#[derive(Deserialize)]
pub(super) struct WithPath {
    person: String,
    fact: String,
    other: String,
}

pub(super) async fn link(
    State(state): State<AppState>,
    editor: Editor,
    Path(WithPath { person, fact, other }): Path<WithPath>,
    Json(body): Json<ToggleWrite>,
) -> Result<StatusCode, AppError> {
    require_id(&person)?;
    require_id(&fact)?;
    require_id(&other)?;
    if person == other {
        return Err(AppError::bad_request("a person cannot be linked to themself"));
    }
    let people = state.db.list_people(&editor)?;
    if !people.iter().any(|item| item.id == person) || !people.iter().any(|item| item.id == other) {
        return Err(AppError::not_found());
    }
    let catalog = state.db.list_personal_facts(&editor)?;
    let claim = catalog
        .iter()
        .find(|item| item.id == fact)
        .map(|item| item.claim.as_str())
        .ok_or_else(AppError::not_found)?;
    if !claim.contains("{other}") {
        return Err(AppError::bad_request("that fact is not about another person"));
    }
    state.db.set_link(&editor, &person, &fact, &other, body.on)?;
    Ok(StatusCode::NO_CONTENT)
}

pub(super) async fn toggle(
    State(state): State<AppState>,
    editor: Editor,
    Path(TogglePath { person, fact }): Path<TogglePath>,
    Json(body): Json<ToggleWrite>,
) -> Result<StatusCode, AppError> {
    state.db.set_toggle(&editor, &person, &fact, body.on)?;
    Ok(StatusCode::NO_CONTENT)
}

pub(super) async fn check_person(
    State(state): State<AppState>,
    scope: Scope,
    Path(CheckPath { id, person: person_id }): Path<CheckPath>,
) -> Result<Json<PersonCheck>, AppError> {
    let assert = state
        .db
        .get_assert(&scope, &id)?
        .ok_or_else(AppError::not_found)?;
    let people = state.db.list_people(&scope)?;
    let person = people
        .iter()
        .find(|item| item.id == person_id)
        .cloned()
        .ok_or_else(AppError::not_found)?;
    let catalog = state.db.list_personal_facts(&scope)?;
    let on: HashSet<String> = person.on.iter().cloned().collect();
    let mut facts = state.db.list_facts(&scope)?;
    let mut shown = Vec::new();
    for item in &catalog {
        if item.claim.contains("{other}") {
            for other in people.iter().filter(|item| item.id != person.id) {
                let chosen = person
                    .links
                    .iter()
                    .any(|link| link.fact == item.id && link.other == other.id);
                if !chosen {
                    continue;
                }
                let id = format!("{}__{}", item.id, other.id);
                if facts.iter().any(|fact| fact.id == id) {
                    continue;
                }
                let claim = item
                    .claim
                    .replace("{name}", &person.name)
                    .replace("{other}", &other.name);
                let fact = Fact {
                    id,
                    claim,
                    citations: Vec::new(),
                    formula: None,
                    role: "fact".into(),
                };
                shown.push(fact.clone());
                facts.push(fact);
            }
            continue;
        }
        let claim = item.claim.replace("{name}", &person.name);
        if on.contains(&item.id) {
            facts.retain(|fact| fact.id != item.id);
            let fact = Fact {
                id: item.id.clone(),
                claim: claim.clone(),
                citations: Vec::new(),
                formula: None,
                role: "fact".into(),
            };
            shown.push(fact.clone());
            facts.push(fact);
        } else if facts.iter().any(|fact| {
            fact.id == item.id && fact.formula.is_none() && fact.role != "criterion"
        }) {
            facts.retain(|fact| fact.id != item.id);
        }
    }
    let rules = state.db.list_rules(&scope)?;
    let mut job = assert_job(scope.ws(), &assert, &facts, &rules);
    job.ephemeral = true;
    let outcome = state.queue.submit(job).await?;
    let (status, graph, diagnostics) = match outcome {
        LeanOutcome::Proved { graph } => ("proved".into(), Some(graph), None),
        LeanOutcome::Invalid { diagnostics } => ("invalid".into(), None, Some(diagnostics)),
    };
    let goal = crate::lean::formula_atoms(&assert.formula);
    let goal = if goal.len() == 1 {
        goal.into_iter().next()
    } else {
        crate::lean::formula_then(&assert.formula)
    };
    let missing = if status == "proved" {
        Vec::new()
    } else if let Some(goal) = goal {
        let given = facts
            .iter()
            .filter(|fact| fact.role == "fact" && fact.formula.is_none())
            .map(|fact| fact.id.clone())
            .collect();
        let theorems = facts.iter().filter_map(|fact| fact.formula.as_deref());
        let off: Vec<String> = catalog
            .iter()
            .filter(|item| !on.contains(&item.id))
            .map(|item| item.id.clone())
            .collect();
        missing(theorems, &given, &off, &goal)
            .unwrap_or_default()
            .into_iter()
            .filter_map(|id| {
                catalog.iter().find(|item| item.id == id).map(|item| PersonalFact {
                    id: item.id.clone(),
                    claim: item.claim.replace("{name}", &person.name),
                })
            })
            .collect()
    } else {
        Vec::new()
    };
    Ok(Json(PersonCheck {
        status,
        graph,
        diagnostics,
        missing,
        facts: shown,
    }))
}
