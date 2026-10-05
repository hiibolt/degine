use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::Json;
use serde::Deserialize;

use crate::access::Editor;
use crate::access::Scope;
use crate::error::AppError;
use crate::events::ServerEvent;
use crate::model::Assert;

use super::{
    clean_text, enqueue_debate, graph_body, id_taken, publish, require_id, AppState, GraphBody,
    IdPath,
};

pub(super) async fn list_asserts(
    State(state): State<AppState>,
    scope: Scope,
) -> Result<Json<Vec<Assert>>, AppError> {
    Ok(Json(state.db.list_asserts(&scope)?))
}

#[derive(Deserialize)]
pub(super) struct AssertWrite {
    title: String,
    description: String,
    formula: String,
    #[serde(default)]
    assumes: Vec<String>,
}

#[derive(Deserialize)]
pub(super) struct NewAssert {
    id: String,
    #[serde(flatten)]
    write: AssertWrite,
}

pub(super) async fn create_assert(
    State(state): State<AppState>,
    editor: Editor,
    Json(body): Json<NewAssert>,
) -> Result<impl IntoResponse, AppError> {
    let assert = assert_from(&state, &editor, body.id, body.write)?;
    if id_taken(&state, &editor, &assert.id)? {
        return Err(AppError::conflict(format!(
            "id `{}` is already used",
            assert.id
        )));
    }
    state.db.insert_assert(&editor, &assert)?;
    state
        .db
        .set_assert_assumes(&editor, &assert.id, &assert.assumes)?;
    let ws = editor.ws().to_string();
    publish(
        &state,
        ServerEvent::AssertChanged {
            workspace_id: ws.clone(),
            assert: assert.clone(),
        },
    );
    enqueue_debate(&state, &ws)?;
    publish(&state, ServerEvent::AccessChanged { workspace_id: ws });
    Ok((StatusCode::CREATED, Json(assert)))
}

pub(super) async fn update_assert(
    State(state): State<AppState>,
    editor: Editor,
    Path(IdPath { id }): Path<IdPath>,
    Json(body): Json<AssertWrite>,
) -> Result<Json<Assert>, AppError> {
    let assert = assert_from(&state, &editor, id, body)?;
    if state.db.fact(&editor, &assert.id)?.is_some()
        || state.db.rule(&editor, &assert.id)?.is_some()
    {
        return Err(AppError::conflict(format!(
            "id `{}` is already used",
            assert.id
        )));
    }
    if !state.db.update_assert(&editor, &assert)? {
        return Err(AppError::not_found());
    }
    state
        .db
        .set_assert_assumes(&editor, &assert.id, &assert.assumes)?;
    let ws = editor.ws().to_string();
    publish(
        &state,
        ServerEvent::AssertChanged {
            workspace_id: ws.clone(),
            assert: assert.clone(),
        },
    );
    enqueue_debate(&state, &ws)?;
    publish(&state, ServerEvent::AccessChanged { workspace_id: ws });
    Ok(Json(assert))
}

pub(super) async fn delete_assert(
    State(state): State<AppState>,
    editor: Editor,
    Path(IdPath { id }): Path<IdPath>,
) -> Result<StatusCode, AppError> {
    state.db.set_assert_assumes(&editor, &id, &[])?;
    if !state.db.delete_assert(&editor, &id)? {
        return Err(AppError::not_found());
    }
    let ws = editor.ws().to_string();
    publish(
        &state,
        ServerEvent::AssertDeleted {
            workspace_id: ws.clone(),
            id,
        },
    );
    enqueue_debate(&state, &ws)?;
    publish(&state, ServerEvent::AccessChanged { workspace_id: ws });
    Ok(StatusCode::NO_CONTENT)
}

pub(super) async fn assert_graph(
    State(state): State<AppState>,
    scope: Scope,
    Path(IdPath { id }): Path<IdPath>,
) -> Result<Json<GraphBody>, AppError> {
    if state.db.get_assert(&scope, &id)?.is_none() {
        return Err(AppError::not_found());
    }
    Ok(Json(graph_body(state.db.assert_graph(&scope, &id)?)))
}

fn assert_from(
    state: &AppState,
    scope: &Scope,
    id: String,
    body: AssertWrite,
) -> Result<Assert, AppError> {
    require_id(&id)?;
    let facts = state.db.list_facts(scope)?;
    let assumes = body
        .assumes
        .into_iter()
        .filter(|id| {
            facts
                .iter()
                .any(|fact| fact.id == *id && fact.role == "criterion" && fact.formula.is_none())
        })
        .collect();
    Ok(Assert {
        id,
        title: clean_text(&body.title, "title")?,
        description: body.description.trim().to_string(),
        formula: clean_text(&body.formula, "formula")?,
        assumes,
    })
}
