use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::Json;
use serde::Deserialize;

use crate::error::AppError;
use crate::events::ServerEvent;
use crate::model::Comment;

use super::{can_see, clean_text, is_owner, publish, AppState, Authed};

#[derive(Deserialize)]
pub(super) struct CommentQuery {
    target_type: String,
    target_id: String,
}

pub(super) async fn list_comments(
    State(state): State<AppState>,
    Authed { id: user_id, email: username }: Authed,
    Query(query): Query<CommentQuery>,
) -> Result<Json<Vec<Comment>>, AppError> {
    let target_id = query.target_id.trim();
    if target_id.is_empty() {
        return Err(AppError::bad_request("target_id is required"));
    }
    ensure_comment_target(&state, &user_id, &username, &query.target_type, target_id)?;
    Ok(Json(state.db.list_comments(&query.target_type, target_id)?))
}

#[derive(Deserialize)]
pub(super) struct CommentBody {
    target_type: String,
    target_id: String,
    body: String,
}

pub(super) async fn create_comment(
    State(state): State<AppState>,
    Authed { id: user_id, email: author }: Authed,
    Json(body): Json<CommentBody>,
) -> Result<impl IntoResponse, AppError> {
    let target_id = clean_text(&body.target_id, "target_id")?;
    let text = clean_text(&body.body, "body")?;
    ensure_comment_target(&state, &user_id, &author, &body.target_type, &target_id)?;
    let comment = state
        .db
        .insert_comment(&body.target_type, &target_id, &author, &text)?;
    publish(
        &state,
        ServerEvent::CommentAdded {
            comment: comment.clone(),
        },
    );
    Ok((StatusCode::CREATED, Json(comment)))
}

#[derive(Deserialize)]
pub(super) struct CommentEdit {
    body: String,
}

pub(super) async fn update_comment(
    State(state): State<AppState>,
    Authed { id: user_id, email: username }: Authed,
    Path(id): Path<i64>,
    Json(body): Json<CommentEdit>,
) -> Result<Json<Comment>, AppError> {
    let existing = state.db.comment(id)?.ok_or_else(AppError::not_found)?;
    if existing.author != username {
        return Err(AppError::forbidden("only the author can edit this"));
    }
    let text = clean_text(&body.body, "body")?;
    let comment = state
        .db
        .update_comment(id, &text)?
        .ok_or_else(AppError::not_found)?;
    publish(
        &state,
        ServerEvent::CommentChanged {
            comment: comment.clone(),
        },
    );
    Ok(Json(comment))
}

pub(super) async fn delete_comment(
    State(state): State<AppState>,
    Authed { id: user_id, email: username }: Authed,
    Path(id): Path<i64>,
) -> Result<StatusCode, AppError> {
    let existing = state.db.comment(id)?.ok_or_else(AppError::not_found)?;
    if existing.author != username && !is_owner(&state, &user_id)? {
        return Err(AppError::forbidden("only the author or the owner can delete this"));
    }
    if !state.db.delete_comment(id)? {
        return Err(AppError::not_found());
    }
    publish(
        &state,
        ServerEvent::CommentDeleted {
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
    Authed { id: user_id, email: username }: Authed,
    Path(id): Path<i64>,
    Json(body): Json<ResolvedBody>,
) -> Result<Json<Comment>, AppError> {
    let existing = state.db.comment(id)?.ok_or_else(AppError::not_found)?;
    if existing.author != username && !is_owner(&state, &user_id)? {
        return Err(AppError::forbidden("only the author or the owner can resolve this"));
    }
    let comment = state
        .db
        .set_comment_resolved(id, body.resolved)?
        .ok_or_else(AppError::not_found)?;
    publish(
        &state,
        ServerEvent::CommentChanged {
            comment: comment.clone(),
        },
    );
    Ok(Json(comment))
}

pub(super) async fn inbox(
    State(state): State<AppState>,
    Authed { id: user_id, email: username }: Authed,
) -> Result<Json<Vec<Comment>>, AppError> {
    let mut visible = Vec::new();
    for comment in state.db.list_open_comments()? {
        if comment.author == username {
            continue;
        }
        if can_see(&state, &user_id, &username, &comment.target_type, &comment.target_id)? {
            visible.push(comment);
        }
    }
    Ok(Json(visible))
}

fn ensure_comment_target(
    state: &AppState,
    user_id: &str,
    username: &str,
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
        "fact" => state.db.fact(target_id)?.is_some(),
        "assert" => state.db.get_assert(target_id)?.is_some(),
        "rule" | "conclusion" => state.db.rule(target_id)?.is_some(),
        _ => false,
    };
    if !exists || !can_see(state, user_id, username, target_type, target_id)? {
        return Err(AppError::not_found());
    }
    Ok(())
}
