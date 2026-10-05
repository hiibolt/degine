use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::Json;
use serde::Deserialize;

use crate::access::{Creator, Scope};
use crate::error::AppError;
use crate::events::ServerEvent;
use crate::model::{Member, Workspace};

use super::{clean_text, publish, valid_username, AppState, Authed};

pub(super) async fn list(
    State(state): State<AppState>,
    Authed { id, .. }: Authed,
) -> Result<Json<Vec<Workspace>>, AppError> {
    let mut workspaces = state.db.list_workspaces(&id)?;
    if workspaces.is_empty() {
        let created = state.db.create_workspace(&id, "library")?;
        workspaces.push(created);
    }
    Ok(Json(workspaces))
}

#[derive(Deserialize)]
pub(super) struct NameBody {
    name: String,
}

pub(super) async fn create(
    State(state): State<AppState>,
    Authed { id, .. }: Authed,
    Json(body): Json<NameBody>,
) -> Result<(StatusCode, Json<Workspace>), AppError> {
    let name = clean_text(&body.name, "name")?;
    if name.chars().count() > 80 {
        return Err(AppError::bad_request("name is too long"));
    }
    let workspace = state.db.create_workspace(&id, &name)?;
    Ok((StatusCode::CREATED, Json(workspace)))
}

pub(super) async fn rename(
    State(state): State<AppState>,
    creator: Creator,
    Json(body): Json<NameBody>,
) -> Result<StatusCode, AppError> {
    let name = clean_text(&body.name, "name")?;
    if name.chars().count() > 80 {
        return Err(AppError::bad_request("name is too long"));
    }
    state.db.rename_workspace(&creator, &name)?;
    publish(
        &state,
        ServerEvent::AccessChanged {
            workspace_id: creator.ws().to_string(),
        },
    );
    Ok(StatusCode::NO_CONTENT)
}

pub(super) async fn delete(
    State(state): State<AppState>,
    creator: Creator,
) -> Result<StatusCode, AppError> {
    let workspace_id = creator.ws().to_string();
    state.db.delete_workspace(&creator)?;
    publish(&state, ServerEvent::AccessChanged { workspace_id });
    Ok(StatusCode::NO_CONTENT)
}

pub(super) async fn leave(
    State(state): State<AppState>,
    scope: Scope,
) -> Result<StatusCode, AppError> {
    let workspace_id = scope.ws().to_string();
    match state.db.leave_workspace(&scope) {
        Ok(()) => {
            publish(&state, ServerEvent::AccessChanged { workspace_id });
            Ok(StatusCode::NO_CONTENT)
        }
        Err(err) if err.to_string() == "creator" => Err(AppError::bad_request(
            "the person who made this workspace can't leave it",
        )),
        Err(err) => Err(err.into()),
    }
}

pub(super) async fn members(
    State(state): State<AppState>,
    scope: Scope,
) -> Result<Json<Vec<Member>>, AppError> {
    Ok(Json(state.db.list_members(&scope)?))
}

#[derive(Deserialize)]
pub(super) struct InviteBody {
    username: String,
    #[serde(default)]
    role: Option<String>,
}

pub(super) async fn add_member(
    State(state): State<AppState>,
    creator: Creator,
    Json(body): Json<InviteBody>,
) -> Result<StatusCode, AppError> {
    let username = body.username.trim().to_lowercase();
    if !valid_username(&username) {
        return Err(AppError::bad_request(
            "username is 3 to 32 letters, numbers, or underscores",
        ));
    }
    let role = body.role.as_deref().unwrap_or("reader");
    if role != "reader" && role != "editor" {
        return Err(AppError::bad_request("role is reader or editor"));
    }
    let Some(user_id) = state.db.user_by_username(&username)? else {
        return Err(AppError::bad_request("no account with that username"));
    };
    if user_id == creator.user_id() {
        return Err(AppError::bad_request("that's you"));
    }
    match state.db.add_member(&creator, &user_id, role) {
        Ok(()) => {
            publish(
                &state,
                ServerEvent::AccessChanged {
                    workspace_id: creator.ws().to_string(),
                },
            );
            Ok(StatusCode::NO_CONTENT)
        }
        Err(err) if err.to_string() == "already" => {
            Err(AppError::conflict("they're already in this workspace"))
        }
        Err(err) => Err(err.into()),
    }
}

#[derive(Deserialize)]
pub(super) struct RoleBody {
    role: String,
}

#[derive(Deserialize)]
pub(super) struct MemberPath {
    user_id: String,
}

pub(super) async fn set_role(
    State(state): State<AppState>,
    creator: Creator,
    Path(MemberPath { user_id }): Path<MemberPath>,
    Json(body): Json<RoleBody>,
) -> Result<StatusCode, AppError> {
    if body.role != "reader" && body.role != "editor" {
        return Err(AppError::bad_request("role is reader or editor"));
    }
    match state.db.set_member_role(&creator, &user_id, &body.role) {
        Ok(()) => {
            publish(
                &state,
                ServerEvent::AccessChanged {
                    workspace_id: creator.ws().to_string(),
                },
            );
            Ok(StatusCode::NO_CONTENT)
        }
        Err(err) if err.to_string() == "creator" => Err(AppError::bad_request(
            "the person who made this workspace stays an editor",
        )),
        Err(err) if err.to_string() == "missing" => Err(AppError::not_found()),
        Err(err) => Err(err.into()),
    }
}

pub(super) async fn remove_member(
    State(state): State<AppState>,
    creator: Creator,
    Path(MemberPath { user_id }): Path<MemberPath>,
) -> Result<StatusCode, AppError> {
    match state.db.remove_member(&creator, &user_id) {
        Ok(()) => {
            publish(
                &state,
                ServerEvent::AccessChanged {
                    workspace_id: creator.ws().to_string(),
                },
            );
            Ok(StatusCode::NO_CONTENT)
        }
        Err(err) if err.to_string() == "creator" => Err(AppError::bad_request(
            "the person who made this workspace stays",
        )),
        Err(err) if err.to_string() == "missing" => Err(AppError::not_found()),
        Err(err) => Err(err.into()),
    }
}
