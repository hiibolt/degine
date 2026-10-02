use std::collections::BTreeMap;

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::Json;
use serde::Deserialize;

use crate::access::grant_for;
use crate::error::AppError;
use crate::events::ServerEvent;
use crate::model::Fact;

use super::{
    blank_to_none, clean_citations, clean_text, enqueue_debate, is_owner, publish, require_id,
    require_owner, AppState, Authed, FactView,
};

pub(super) async fn list_facts(
    State(state): State<AppState>,
    Authed { id: user_id, email: username }: Authed,
) -> Result<Json<Vec<FactView>>, AppError> {
    let facts = state.db.list_facts()?;
    if is_owner(&state, &user_id)? {
        return Ok(Json(
            facts
                .into_iter()
                .map(|fact| FactView { fact, shared: false })
                .collect(),
        ));
    }
    let grant = grant_for(&state.db, &username)?;
    Ok(Json(
        facts
            .into_iter()
            .filter(|fact| grant.fact_ids.contains(&fact.id))
            .map(|fact| FactView { fact, shared: true })
            .collect(),
    ))
}

#[derive(Deserialize)]
pub(super) struct FactWrite {
    claim: String,
    citations: Vec<String>,
    formula: Option<String>,
    #[serde(default)]
    role: Option<String>,
}

#[derive(Deserialize)]
pub(super) struct NewFact {
    id: String,
    #[serde(flatten)]
    write: FactWrite,
}

pub(super) async fn create_fact(
    State(state): State<AppState>,
    Authed { id: user_id, email: username }: Authed,
    Json(body): Json<NewFact>,
) -> Result<impl IntoResponse, AppError> {
    require_owner(&state, &user_id)?;
    let fact = fact_from(body.id, body.write)?;
    if state.db.fact(&fact.id)?.is_some() || state.db.rule(&fact.id)?.is_some() {
        return Err(AppError::conflict(format!("id `{}` is already used", fact.id)));
    }
    state.db.insert_fact(&fact)?;
    publish(&state, ServerEvent::FactChanged { fact: fact.clone() });
    enqueue_debate(&state)?;
    publish(&state, ServerEvent::AccessChanged);
    Ok((StatusCode::CREATED, Json(fact)))
}

pub(super) async fn update_fact(
    State(state): State<AppState>,
    Authed { id: user_id, email: username }: Authed,
    Path(id): Path<String>,
    Json(body): Json<FactWrite>,
) -> Result<Json<Fact>, AppError> {
    require_owner(&state, &user_id)?;
    let fact = fact_from(id, body)?;
    if state.db.rule(&fact.id)?.is_some() {
        return Err(AppError::conflict(format!(
            "id `{}` is already used by a rule",
            fact.id
        )));
    }
    if !state.db.update_fact(&fact)? {
        return Err(AppError::not_found());
    }
    publish(&state, ServerEvent::FactChanged { fact: fact.clone() });
    enqueue_debate(&state)?;
    publish(&state, ServerEvent::AccessChanged);
    Ok(Json(fact))
}

pub(super) async fn delete_fact(
    State(state): State<AppState>,
    Authed { id: user_id, email: username }: Authed,
    Path(id): Path<String>,
) -> Result<StatusCode, AppError> {
    require_owner(&state, &user_id)?;
    if !state.db.delete_fact(&id)? {
        return Err(AppError::not_found());
    }
    publish(&state, ServerEvent::FactDeleted { id });
    enqueue_debate(&state)?;
    publish(&state, ServerEvent::AccessChanged);
    Ok(StatusCode::NO_CONTENT)
}

pub(super) async fn list_labels(
    State(state): State<AppState>,
    Authed { id: user_id, email: username }: Authed,
) -> Result<Json<BTreeMap<String, String>>, AppError> {
    let labels = state.db.list_labels()?;
    if is_owner(&state, &user_id)? {
        return Ok(Json(labels));
    }
    let grant = grant_for(&state.db, &username)?;
    Ok(Json(
        labels
            .into_iter()
            .filter(|(id, _)| grant.atoms.contains(id))
            .collect(),
    ))
}

pub(super) async fn put_labels(
    State(state): State<AppState>,
    Authed { id: user_id, email: username }: Authed,
    Json(body): Json<BTreeMap<String, String>>,
) -> Result<StatusCode, AppError> {
    require_owner(&state, &user_id)?;
    state.db.upsert_labels(&body)?;
    Ok(StatusCode::NO_CONTENT)
}

fn fact_from(id: String, body: FactWrite) -> Result<Fact, AppError> {
    require_id(&id)?;
    let formula = blank_to_none(body.formula);
    let requested = body.role.as_deref();
    let role = match (requested, formula.is_some()) {
        (Some("criterion"), false) => "criterion",
        (Some("fact") | None, false) => "fact",
        (Some("theorem") | Some("fact") | None, true) => "theorem",
        (Some(other), _) => {
            return Err(AppError::bad_request(format!(
                "role `{other}` does not match the formula"
            )));
        }
    };
    let citations = if role == "criterion" {
        Vec::new()
    } else {
        clean_citations(body.citations)
    };
    Ok(Fact {
        id,
        claim: clean_text(&body.claim, "claim")?,
        citations,
        formula,
        role: role.to_string(),
    })
}
