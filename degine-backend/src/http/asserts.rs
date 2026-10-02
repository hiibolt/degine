use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::Json;
use serde::Deserialize;

use crate::access::grant_for;
use crate::error::AppError;
use crate::events::ServerEvent;
use crate::model::Assert;

use super::{
    can_see, clean_text, enqueue_debate, graph_body, id_taken, is_owner, publish, require_id,
    require_owner, AppState, AssertView, Authed, GraphBody,
};

pub(super) async fn list_asserts(
    State(state): State<AppState>,
    Authed { id: user_id, email: username }: Authed,
) -> Result<Json<Vec<AssertView>>, AppError> {
    let asserts = state.db.list_asserts()?;
    if is_owner(&state, &user_id)? {
        return Ok(Json(
            asserts
                .into_iter()
                .map(|assert| AssertView {
                    assert,
                    shared: false,
                })
                .collect(),
        ));
    }
    let grant = grant_for(&state.db, &username)?;
    Ok(Json(
        asserts
            .into_iter()
            .filter(|assert| grant.assert_ids.contains(&assert.id))
            .map(|assert| AssertView {
                assert,
                shared: true,
            })
            .collect(),
    ))
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
    Authed { id: user_id, email: _username }: Authed,
    Json(body): Json<NewAssert>,
) -> Result<impl IntoResponse, AppError> {
    require_owner(&state, &user_id)?;
    let assert = assert_from(&state, body.id, body.write)?;
    if id_taken(&state, &assert.id)? {
        return Err(AppError::conflict(format!("id `{}` is already used", assert.id)));
    }
    state.db.insert_assert(&assert)?;
    state.db.set_assert_assumes(&assert.id, &assert.assumes)?;
    publish(
        &state,
        ServerEvent::AssertChanged {
            assert: assert.clone(),
        },
    );
    enqueue_debate(&state)?;
    publish(&state, ServerEvent::AccessChanged);
    Ok((StatusCode::CREATED, Json(assert)))
}

pub(super) async fn update_assert(
    State(state): State<AppState>,
    Authed { id: user_id, email: _username }: Authed,
    Path(id): Path<String>,
    Json(body): Json<AssertWrite>,
) -> Result<Json<Assert>, AppError> {
    require_owner(&state, &user_id)?;
    let assert = assert_from(&state, id, body)?;
    if state.db.fact(&assert.id)?.is_some() || state.db.rule(&assert.id)?.is_some() {
        return Err(AppError::conflict(format!("id `{}` is already used", assert.id)));
    }
    if !state.db.update_assert(&assert)? {
        return Err(AppError::not_found());
    }
    state.db.set_assert_assumes(&assert.id, &assert.assumes)?;
    publish(
        &state,
        ServerEvent::AssertChanged {
            assert: assert.clone(),
        },
    );
    enqueue_debate(&state)?;
    publish(&state, ServerEvent::AccessChanged);
    Ok(Json(assert))
}

pub(super) async fn delete_assert(
    State(state): State<AppState>,
    Authed { id: user_id, email: _username }: Authed,
    Path(id): Path<String>,
) -> Result<StatusCode, AppError> {
    require_owner(&state, &user_id)?;
    state.db.set_assert_assumes(&id, &[])?;
    if !state.db.delete_assert(&id)? {
        return Err(AppError::not_found());
    }
    publish(&state, ServerEvent::AssertDeleted { id });
    enqueue_debate(&state)?;
    publish(&state, ServerEvent::AccessChanged);
    Ok(StatusCode::NO_CONTENT)
}

pub(super) async fn assert_graph(
    State(state): State<AppState>,
    Authed { id: user_id, email: username }: Authed,
    Path(id): Path<String>,
) -> Result<Json<GraphBody>, AppError> {
    if state.db.get_assert(&id)?.is_none() || !can_see(&state, &user_id, &username, "assert", &id)? {
        return Err(AppError::not_found());
    }
    Ok(Json(graph_body(state.db.assert_graph(&id)?)))
}

fn assert_from(state: &AppState, id: String, body: AssertWrite) -> Result<Assert, AppError> {
    require_id(&id)?;
    let facts = state.db.list_facts()?;
    let assumes = body
        .assumes
        .into_iter()
        .filter(|id| {
            facts.iter().any(|fact| {
                fact.id == *id && fact.role == "criterion" && fact.formula.is_none()
            })
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
