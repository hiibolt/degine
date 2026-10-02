use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::Json;
use serde::Deserialize;

use crate::error::AppError;
use crate::events::ServerEvent;
use crate::model::Rule;

use super::{
    clean_text, enqueue_debate, graph_body, is_owner, publish, require_id, require_owner, AppState,
    Authed, GraphBody,
};

pub(super) async fn list_rules(
    State(state): State<AppState>,
    Authed { id: user_id, email: username }: Authed,
) -> Result<Json<Vec<Rule>>, AppError> {
    if is_owner(&state, &user_id)? {
        Ok(Json(state.db.list_rules()?))
    } else {
        Ok(Json(Vec::new()))
    }
}

#[derive(Deserialize)]
pub(super) struct RuleWrite {
    premises: Vec<String>,
    conclusion: String,
}

#[derive(Deserialize)]
pub(super) struct NewRule {
    id: String,
    #[serde(flatten)]
    write: RuleWrite,
}

pub(super) async fn create_rule(
    State(state): State<AppState>,
    Authed { id: user_id, email: username }: Authed,
    Json(body): Json<NewRule>,
) -> Result<impl IntoResponse, AppError> {
    require_owner(&state, &user_id)?;
    let rule = rule_from(body.id, body.write)?;
    if state.db.fact(&rule.id)?.is_some() || state.db.rule(&rule.id)?.is_some() {
        return Err(AppError::conflict(format!("id `{}` is already used", rule.id)));
    }
    state.db.insert_rule(&rule)?;
    publish(&state, ServerEvent::RuleChanged { rule: rule.clone() });
    enqueue_debate(&state)?;
    publish(&state, ServerEvent::AccessChanged);
    Ok((StatusCode::CREATED, Json(rule)))
}

pub(super) async fn update_rule(
    State(state): State<AppState>,
    Authed { id: user_id, email: username }: Authed,
    Path(id): Path<String>,
    Json(body): Json<RuleWrite>,
) -> Result<Json<Rule>, AppError> {
    require_owner(&state, &user_id)?;
    let rule = rule_from(id, body)?;
    if state.db.fact(&rule.id)?.is_some() {
        return Err(AppError::conflict(format!(
            "id `{}` is already used by a fact",
            rule.id
        )));
    }
    if !state.db.update_rule(&rule)? {
        return Err(AppError::not_found());
    }
    publish(&state, ServerEvent::RuleChanged { rule: rule.clone() });
    enqueue_debate(&state)?;
    publish(&state, ServerEvent::AccessChanged);
    Ok(Json(rule))
}

pub(super) async fn delete_rule(
    State(state): State<AppState>,
    Authed { id: user_id, email: username }: Authed,
    Path(id): Path<String>,
) -> Result<StatusCode, AppError> {
    require_owner(&state, &user_id)?;
    if !state.db.delete_rule(&id)? {
        return Err(AppError::not_found());
    }
    publish(&state, ServerEvent::RuleDeleted { id });
    enqueue_debate(&state)?;
    publish(&state, ServerEvent::AccessChanged);
    Ok(StatusCode::NO_CONTENT)
}

pub(super) async fn rule_graph(
    State(state): State<AppState>,
    Authed { id: user_id, email: username }: Authed,
    Path(id): Path<String>,
) -> Result<Json<GraphBody>, AppError> {
    if !is_owner(&state, &user_id)? || state.db.rule(&id)?.is_none() {
        return Err(AppError::not_found());
    }
    Ok(Json(graph_body(state.db.graph(&id)?)))
}

fn rule_from(id: String, body: RuleWrite) -> Result<Rule, AppError> {
    require_id(&id)?;
    let premises = body
        .premises
        .into_iter()
        .map(|premise| {
            let premise = premise.trim().to_string();
            if premise.is_empty() {
                Err(AppError::bad_request("a premise id is empty"))
            } else {
                Ok(premise)
            }
        })
        .collect::<Result<Vec<_>, _>>()?;
    Ok(Rule {
        id,
        premises,
        conclusion: clean_text(&body.conclusion, "conclusion")?,
    })
}
