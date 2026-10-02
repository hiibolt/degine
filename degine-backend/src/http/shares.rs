use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::Json;
use serde::Deserialize;

use crate::error::AppError;
use crate::events::ServerEvent;

use super::{publish, require_owner, AppState, Authed};

#[derive(Deserialize)]
pub(super) struct ShareBody {
    email: String,
}

pub(super) async fn list_shares(
    State(state): State<AppState>,
    Authed { id: user_id, email: _username }: Authed,
    Path(id): Path<String>,
) -> Result<Json<Vec<String>>, AppError> {
    require_owner(&state, &user_id)?;
    if state.db.get_assert(&id)?.is_none() {
        return Err(AppError::not_found());
    }
    Ok(Json(state.db.shares_of(&id)?))
}

pub(super) async fn grant_share(
    State(state): State<AppState>,
    Authed { id: user_id, email: username }: Authed,
    Path(id): Path<String>,
    Json(body): Json<ShareBody>,
) -> Result<StatusCode, AppError> {
    require_owner(&state, &user_id)?;
    if state.db.get_assert(&id)?.is_none() {
        return Err(AppError::not_found());
    }
    let grantee = body.email.trim().to_lowercase();
    if !grantee.contains('@') || grantee == username {
        return Err(AppError::bad_request("enter an email"));
    }
    state.db.grant_share(&id, &grantee)?;
    publish(&state, ServerEvent::AccessChanged);
    Ok(StatusCode::NO_CONTENT)
}

pub(super) async fn revoke_share(
    State(state): State<AppState>,
    Authed { id: user_id, email: _username }: Authed,
    Path((id, grantee)): Path<(String, String)>,
) -> Result<StatusCode, AppError> {
    require_owner(&state, &user_id)?;
    if state.db.get_assert(&id)?.is_none() {
        return Err(AppError::not_found());
    }
    state.db.revoke_share(&id, &grantee)?;
    publish(&state, ServerEvent::AccessChanged);
    Ok(StatusCode::NO_CONTENT)
}
