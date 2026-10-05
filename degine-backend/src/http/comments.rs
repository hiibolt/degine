use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::Json;
use serde::Deserialize;

use crate::access::Scope;
use crate::error::AppError;
use crate::events::ServerEvent;
use crate::model::Comment;

use super::{clean_text, publish, AppState};

#[derive(Deserialize)]
pub(super) struct CommentQuery {
    target_type: String,
    target_id: String,
}

pub(super) async fn list_comments(
    State(state): State<AppState>,
    scope: Scope,
    Query(query): Query<CommentQuery>,
) -> Result<Json<Vec<Comment>>, AppError> {
    let target_id = query.target_id.trim();
    if target_id.is_empty() {
        return Err(AppError::bad_request("target_id is required"));
    }
    ensure_comment_target(&state, &scope, &query.target_type, target_id)?;
    Ok(Json(state.db.list_comments(&scope, &query.target_type, target_id)?))
}

#[derive(Deserialize)]
pub(super) struct CommentBody {
    target_type: String,
    target_id: String,
    body: String,
}

pub(super) async fn create_comment(
    State(state): State<AppState>,
    scope: Scope,
    Json(body): Json<CommentBody>,
) -> Result<impl IntoResponse, AppError> {
    let target_id = clean_text(&body.target_id, "target_id")?;
    let text = clean_text(&body.body, "body")?;
    ensure_comment_target(&state, &scope, &body.target_type, &target_id)?;
    let author = if scope.username().is_empty() {
        scope.email().to_string()
    } else {
        scope.username().to_string()
    };
    let comment = state
        .db
        .insert_comment(&scope, &body.target_type, &target_id, &author, &text)?;
    publish(
        &state,
        ServerEvent::CommentAdded {
            workspace_id: scope.ws().to_string(),
            comment: comment.clone(),
        },
    );
    Ok((StatusCode::CREATED, Json(comment)))
}

#[derive(Deserialize)]
pub(super) struct CommentEdit {
    body: String,
}

#[derive(Deserialize)]
pub(super) struct CommentPath {
    id: i64,
}

pub(super) async fn update_comment(
    State(state): State<AppState>,
    scope: Scope,
    Path(CommentPath { id }): Path<CommentPath>,
    Json(body): Json<CommentEdit>,
) -> Result<Json<Comment>, AppError> {
    let existing = state.db.comment(&scope, id)?.ok_or_else(AppError::not_found)?;
    if !same_author(&scope, &existing.author) {
        return Err(AppError::forbidden("only the author can edit this"));
    }
    let text = clean_text(&body.body, "body")?;
    let comment = state
        .db
        .update_comment(&scope, id, &text)?
        .ok_or_else(AppError::not_found)?;
    publish(
        &state,
        ServerEvent::CommentChanged {
            workspace_id: scope.ws().to_string(),
            comment: comment.clone(),
        },
    );
    Ok(Json(comment))
}

pub(super) async fn delete_comment(
    State(state): State<AppState>,
    scope: Scope,
    Path(CommentPath { id }): Path<CommentPath>,
) -> Result<StatusCode, AppError> {
    let existing = state.db.comment(&scope, id)?.ok_or_else(AppError::not_found)?;
    if !same_author(&scope, &existing.author) && !scope.editor() {
        return Err(AppError::forbidden("only the author or an editor can delete this"));
    }
    if !state.db.delete_comment(&scope, id)? {
        return Err(AppError::not_found());
    }
    publish(
        &state,
        ServerEvent::CommentDeleted {
            workspace_id: scope.ws().to_string(),
            id,
            target_type: existing.target_type,
            target_id: existing.target_id,
        },
    );
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Deserialize)]
pub(super) struct ResolvedBody {
    resolved: bool,
}

pub(super) async fn resolve_comment(
    State(state): State<AppState>,
    scope: Scope,
    Path(CommentPath { id }): Path<CommentPath>,
    Json(body): Json<ResolvedBody>,
) -> Result<Json<Comment>, AppError> {
    let existing = state.db.comment(&scope, id)?.ok_or_else(AppError::not_found)?;
    if !same_author(&scope, &existing.author) && !scope.editor() {
        return Err(AppError::forbidden("only the author or an editor can resolve this"));
    }
    let comment = state
        .db
        .set_comment_resolved(&scope, id, body.resolved)?
        .ok_or_else(AppError::not_found)?;
    publish(
        &state,
        ServerEvent::CommentChanged {
            workspace_id: scope.ws().to_string(),
            comment: comment.clone(),
        },
    );
    Ok(Json(comment))
}

pub(super) async fn inbox(
    State(state): State<AppState>,
    scope: Scope,
) -> Result<Json<Vec<Comment>>, AppError> {
    let visible = state
        .db
        .list_open_comments(&scope)?
        .into_iter()
        .filter(|comment| !same_author(&scope, &comment.author))
        .collect();
    Ok(Json(visible))
}

fn same_author(scope: &Scope, author: &str) -> bool {
    author == scope.username() || author == scope.email()
}

fn ensure_comment_target(
    state: &AppState,
    scope: &Scope,
    target_type: &str,
    target_id: &str,
) -> Result<(), AppError> {
    match target_type {
        "fact" | "assert" | "rule" | "conclusion" => {}
        _ => {
            return Err(AppError::bad_request(
                "target_type must be fact, assert, rule, or conclusion",
            ));
        }
    }
    let exists = match target_type {
        "fact" => state.db.fact(scope, target_id)?.is_some(),
        "assert" => state.db.get_assert(scope, target_id)?.is_some(),
        "rule" | "conclusion" => state.db.rule(scope, target_id)?.is_some(),
        _ => false,
    };
    if !exists {
        return Err(AppError::not_found());
    }
    Ok(())
}
