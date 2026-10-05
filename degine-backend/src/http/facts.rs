use std::collections::BTreeMap;

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::Json;
use serde::Deserialize;

use crate::access::{Editor, Scope};
use crate::error::AppError;
use crate::events::ServerEvent;
use crate::model::Fact;

use super::{
    blank_to_none, clean_citations, clean_text, enqueue_debate, publish, require_id, AppState,
    IdPath,
};

pub(super) async fn list_facts(
    State(state): State<AppState>,
    scope: Scope,
) -> Result<Json<Vec<Fact>>, AppError> {
    Ok(Json(state.db.list_facts(&scope)?))
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
    editor: Editor,
    Json(body): Json<NewFact>,
) -> Result<impl IntoResponse, AppError> {
    let fact = fact_from(body.id, body.write)?;
    if state.db.fact(&editor, &fact.id)?.is_some() || state.db.rule(&editor, &fact.id)?.is_some() {
        return Err(AppError::conflict(format!("id `{}` is already used", fact.id)));
    }
    state.db.insert_fact(&editor, &fact)?;
    let ws = editor.ws().to_string();
    publish(
        &state,
        ServerEvent::FactChanged {
            workspace_id: ws.clone(),
            fact: fact.clone(),
        },
    );
    enqueue_debate(&state, &ws)?;
    publish(&state, ServerEvent::AccessChanged { workspace_id: ws });
    Ok((StatusCode::CREATED, Json(fact)))
}

pub(super) async fn update_fact(
    State(state): State<AppState>,
    editor: Editor,
    Path(IdPath { id }): Path<IdPath>,
    Json(body): Json<FactWrite>,
) -> Result<Json<Fact>, AppError> {
    let fact = fact_from(id, body)?;
    if state.db.rule(&editor, &fact.id)?.is_some() {
        return Err(AppError::conflict(format!(
            "id `{}` is already used by a rule",
            fact.id
        )));
    }
    if !state.db.update_fact(&editor, &fact)? {
        return Err(AppError::not_found());
    }
    let ws = editor.ws().to_string();
    publish(
        &state,
        ServerEvent::FactChanged {
            workspace_id: ws.clone(),
            fact: fact.clone(),
        },
    );
    enqueue_debate(&state, &ws)?;
    publish(&state, ServerEvent::AccessChanged { workspace_id: ws });
    Ok(Json(fact))
}

#[derive(Deserialize)]
pub(super) struct DeriveBody {
    claim: String,
    id: String,
}

pub(super) async fn derive_fact(
    State(state): State<AppState>,
    editor: Editor,
    Path(IdPath { id }): Path<IdPath>,
    Json(body): Json<DeriveBody>,
) -> Result<Json<Fact>, AppError> {
    require_id(&body.id)?;
    let claim = clean_text(&body.claim, "claim")?;
    if body.id == id {
        return Err(AppError::bad_request("the new fact needs its own id"));
    }
    match state.db.derive_fact(&editor, &id, &body.id, &claim) {
        Ok(theorem) => {
            let ws = editor.ws().to_string();
            publish(
                &state,
                ServerEvent::FactDeleted {
                    workspace_id: ws.clone(),
                    id: id.clone(),
                },
            );
            publish(
                &state,
                ServerEvent::FactChanged {
                    workspace_id: ws.clone(),
                    fact: Fact {
                        id: body.id,
                        claim: claim.clone(),
                        citations: Vec::new(),
                        formula: None,
                        role: "fact".into(),
                    },
                },
            );
            publish(
                &state,
                ServerEvent::FactChanged {
                    workspace_id: ws.clone(),
                    fact: theorem.clone(),
                },
            );
            enqueue_debate(&state, &ws)?;
            Ok(Json(theorem))
        }
        Err(err) => match err.to_string().as_str() {
            "missing" => Err(AppError::not_found()),
            "not a fact" => Err(AppError::bad_request("only a fact can become a theorem")),
            "taken" => Err(AppError::conflict("that id is already used")),
            _ => Err(err.into()),
        },
    }
}

pub(super) async fn delete_fact(
    State(state): State<AppState>,
    editor: Editor,
    Path(IdPath { id }): Path<IdPath>,
) -> Result<StatusCode, AppError> {
    if !state.db.delete_fact(&editor, &id)? {
        return Err(AppError::not_found());
    }
    let ws = editor.ws().to_string();
    publish(
        &state,
        ServerEvent::FactDeleted {
            workspace_id: ws.clone(),
            id,
        },
    );
    enqueue_debate(&state, &ws)?;
    publish(&state, ServerEvent::AccessChanged { workspace_id: ws });
    Ok(StatusCode::NO_CONTENT)
}

pub(super) async fn list_labels(
    State(state): State<AppState>,
    scope: Scope,
) -> Result<Json<BTreeMap<String, String>>, AppError> {
    Ok(Json(state.db.list_labels(&scope)?))
}

pub(super) async fn delete_label(
    State(state): State<AppState>,
    editor: Editor,
    Path(IdPath { id }): Path<IdPath>,
) -> Result<StatusCode, AppError> {
    require_id(&id)?;
    match state.db.delete_label(&editor, &id) {
        Ok(Some(())) => {
            publish(
                &state,
                ServerEvent::AccessChanged {
                    workspace_id: editor.ws().to_string(),
                },
            );
            Ok(StatusCode::NO_CONTENT)
        }
        Ok(None) => Err(AppError::not_found()),
        Err(err) if err.to_string() == "in use" => {
            Err(AppError::conflict("that label is still used in a formula"))
        }
        Err(err) => Err(err.into()),
    }
}

pub(super) async fn put_labels(
    State(state): State<AppState>,
    editor: Editor,
    Json(body): Json<BTreeMap<String, String>>,
) -> Result<StatusCode, AppError> {
    state.db.upsert_labels(&editor, &body)?;
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
