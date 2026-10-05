use axum::extract::ws::{Message, WebSocket};
use tokio::sync::broadcast;
use axum::extract::{Query, State, WebSocketUpgrade};
use axum::response::IntoResponse;
use axum::Json;
use serde::{Deserialize, Serialize};

use crate::error::AppError;
use crate::events::ServerEvent;

use super::AppState;
use super::Authed;

#[derive(Serialize)]
pub(super) struct MeBody {
    username: String,
}

#[derive(Serialize)]
pub(super) struct TokenBody {
    token: String,
}

pub(super) async fn show_token(
    Authed { id: user_id, .. }: Authed,
    State(state): State<AppState>,
) -> Result<Json<TokenBody>, AppError> {
    Ok(Json(TokenBody {
        token: state.db.api_token(&user_id)?,
    }))
}

pub(super) async fn reset_token(
    Authed { id: user_id, .. }: Authed,
    State(state): State<AppState>,
) -> Result<Json<TokenBody>, AppError> {
    Ok(Json(TokenBody {
        token: state.db.reset_api_token(&user_id)?,
    }))
}

pub(super) async fn me(Authed { username, .. }: Authed) -> Result<Json<MeBody>, AppError> {
    Ok(Json(MeBody { username }))
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
    let user_id = person.id;
    Ok(upgrade.on_upgrade(move |socket| run_socket(socket, state, user_id)))
}

fn can_hear(state: &AppState, user_id: &str, event: &ServerEvent) -> Result<bool, AppError> {
    if matches!(event, ServerEvent::AccessChanged { .. }) {
        return Ok(true);
    }
    state.db.is_member(user_id, event.workspace_id()).map_err(AppError::from)
}

async fn run_socket(mut socket: WebSocket, state: AppState, user_id: String) {
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
                        let allow = match can_hear(&state, &user_id, &event) {
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
