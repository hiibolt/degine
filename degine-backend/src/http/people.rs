use std::collections::HashSet;

use axum::extract::{Path, State};
use axum::Json;
use serde::{Deserialize, Serialize};

use crate::lean::{missing, DepGraph, LeanOutcome};
use crate::model::{Fact, Person, PersonalFact};

use super::{assert_job, can_see, clean_text, require_id, require_owner, AppError, AppState, Authed};

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
    Authed { .. }: Authed,
) -> Result<Json<Vec<Person>>, AppError> {
    Ok(Json(state.db.list_people()?))
}

pub(super) async fn create_person(
    State(state): State<AppState>,
    Authed { id: user_id, .. }: Authed,
    Json(body): Json<PersonWrite>,
) -> Result<Json<Person>, AppError> {
    require_owner(&state, &user_id)?;
    require_id(&body.id)?;
    let name = clean_text(&body.name, "name")?;
    state.db.insert_person(&body.id, &name)?;
    Ok(Json(Person {
        id: body.id,
        name,
        on: Vec::new(),
    }))
}

pub(super) async fn delete_person(
    State(state): State<AppState>,
    Authed { id: user_id, .. }: Authed,
    Path(id): Path<String>,
) -> Result<StatusCode, AppError> {
    require_owner(&state, &user_id)?;
    if !state.db.delete_person(&id)? {
        return Err(AppError::not_found());
    }
    Ok(StatusCode::NO_CONTENT)
}

use axum::http::StatusCode;

pub(super) async fn list_personal(
    State(state): State<AppState>,
    Authed { .. }: Authed,
) -> Result<Json<Vec<PersonalFact>>, AppError> {
    Ok(Json(state.db.list_personal_facts()?))
}

pub(super) async fn create_personal(
    State(state): State<AppState>,
    Authed { id: user_id, .. }: Authed,
    Json(body): Json<PersonalWrite>,
) -> Result<Json<PersonalFact>, AppError> {
    require_owner(&state, &user_id)?;
    require_id(&body.id)?;
    let claim = clean_text(&body.claim, "claim")?;
    state.db.insert_personal_fact(&body.id, &claim)?;
    Ok(Json(PersonalFact { id: body.id, claim }))
}

pub(super) async fn delete_personal(
    State(state): State<AppState>,
    Authed { id: user_id, .. }: Authed,
    Path(id): Path<String>,
) -> Result<StatusCode, AppError> {
    require_owner(&state, &user_id)?;
    if !state.db.delete_personal_fact(&id)? {
        return Err(AppError::not_found());
    }
    Ok(StatusCode::NO_CONTENT)
}

pub(super) async fn toggle(
    State(state): State<AppState>,
    Authed { id: user_id, .. }: Authed,
    Path((person, fact)): Path<(String, String)>,
    Json(body): Json<ToggleWrite>,
) -> Result<StatusCode, AppError> {
    require_owner(&state, &user_id)?;
    state.db.set_toggle(&person, &fact, body.on)?;
    Ok(StatusCode::NO_CONTENT)
}

pub(super) async fn check_person(
    State(state): State<AppState>,
    Authed { id: user_id, email: username }: Authed,
    Path((assert_id, person_id)): Path<(String, String)>,
) -> Result<Json<PersonCheck>, AppError> {
    let assert = state
        .db
        .get_assert(&assert_id)?
        .ok_or_else(AppError::not_found)?;
    if !can_see(&state, &user_id, &username, "assert", &assert_id)? {
        return Err(AppError::not_found());
    }
    let people = state.db.list_people()?;
    let person = people
        .into_iter()
        .find(|item| item.id == person_id)
        .ok_or_else(AppError::not_found)?;
    let catalog = state.db.list_personal_facts()?;
    let on: HashSet<String> = person.on.iter().cloned().collect();
    let mut facts = state.db.list_facts()?;
    let mut shown = Vec::new();
    for item in &catalog {
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
    let rules = state.db.list_rules()?;
    let mut job = assert_job(&assert, &facts, &rules);
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
