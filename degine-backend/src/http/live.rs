use axum::extract::ws::{Message, WebSocket};
use tokio::sync::broadcast;
use axum::extract::{Query, State, WebSocketUpgrade};
use axum::response::IntoResponse;
use axum::Json;
use serde::{Deserialize, Serialize};

use crate::access::grant_for;
use crate::error::AppError;
use crate::events::ServerEvent;

use super::{is_owner, AppState, Authed};

#[derive(Serialize)]
pub(super) struct MeBody {
    username: String,
    owner: bool,
}

pub(super) async fn me(
    Authed { id: user_id, email: username }: Authed,
    State(state): State<AppState>,
) -> Result<Json<MeBody>, AppError> {
    Ok(Json(MeBody {
        username: username.clone(),
        owner: is_owner(&state, &user_id)?,
    }))
}

#[derive(Deserialize)]
pub(super) struct TokenQuery {
    token: String,
}

pub(super) async fn ws(
    State(state): State<AppState>,
    Query(query): Query<TokenQuery>,
    upgrade: WebSocketUpgrade,
) -> Result<impl IntoResponse, AppError> {
    let person = state
        .auth
        .verify(&query.token)
        .await
        .map_err(|_| AppError::unauthorized("unauthorized"))?;
    state.db.claim_owner(&person.id)?;
    let user_id = person.id;
    let email = person.email;
    Ok(upgrade.on_upgrade(move |socket| run_socket(socket, state, user_id, email)))
}

fn guest_sees(
    state: &AppState,
    user_id: &str,
    email: &str,
    event: &ServerEvent,
) -> Result<bool, AppError> {
    if is_owner(state, user_id)? {
        return Ok(true);
    }
    match event {
        ServerEvent::FactDeleted { .. }
        | ServerEvent::AssertDeleted { .. }
        | ServerEvent::RuleDeleted { .. }
        | ServerEvent::CommentDeleted { .. }
        | ServerEvent::AccessChanged => Ok(true),
        ServerEvent::RuleChanged { .. } => Ok(false),
        ServerEvent::FactChanged { fact } => {
            let grant = grant_for(&state.db, email)?;
            Ok(grant.fact_ids.contains(&fact.id))
        }
        ServerEvent::AssertChanged { assert } => {
            let grant = grant_for(&state.db, email)?;
            Ok(grant.assert_ids.contains(&assert.id))
        }
        ServerEvent::CommentAdded { comment } | ServerEvent::CommentChanged { comment } => {
            let grant = grant_for(&state.db, email)?;
            Ok(grant.sees_target(&comment.target_type, &comment.target_id))
        }
        ServerEvent::CompileStarted { target_rule_id }
        | ServerEvent::GraphUpdated { target_rule_id, .. }
        | ServerEvent::CompileFailed { target_rule_id, .. } => {
            let grant = grant_for(&state.db, email)?;
            Ok(grant.assert_ids.contains(target_rule_id))
        }
    }
}

async fn run_socket(mut socket: WebSocket, state: AppState, user_id: String, email: String) {
    let mut events = state.events.subscribe();
    loop {
        tokio::select! {
            incoming = socket.recv() => {
                match incoming {
                    None | Some(Ok(Message::Close(_))) => break,
                    Some(Err(err)) => {
                        tracing::warn!("websocket read failed: {err}");
                        break;
                    }
                    Some(Ok(_)) => {}
                }
            }
            event = events.recv() => {
                match event {
                    Ok(event) => {
                        let allow = match guest_sees(&state, &user_id, &email, &event) {
                            Ok(allow) => allow,
                            Err(err) => {
                                tracing::error!("could not filter a websocket event: {err:?}");
                                false
                            }
                        };
                        if !allow {
                            continue;
                        }
                        let text = match serde_json::to_string(&event) {
                            Ok(text) => text,
                            Err(err) => {
                                tracing::error!("failed to encode a websocket event: {err:#}");
                                break;
                            }
                        };
                        if socket.send(Message::Text(text.into())).await.is_err() {
                            break;
                        }
                    }
                    Err(broadcast::error::RecvError::Lagged(skipped)) => {
                        tracing::warn!("websocket lagged by {skipped} events");
                    }
                    Err(broadcast::error::RecvError::Closed) => break,
                }
            }
        }
    }
}
