mod asserts;
mod comments;
mod facts;
mod live;
mod people;
mod rules;
mod workspaces;

use anyhow::{Context, Result};
use axum::extract::{FromRequestParts, Path};
use axum::http::request::Parts;
use axum::routing::{get, post, put};
use axum::Router;
use serde::{Deserialize, Serialize};
use tokio::net::TcpListener;
use tokio::sync::broadcast;
use tower_http::cors::CorsLayer;

use crate::access::{Creator, Editor, Scope};
use crate::auth::Auth;
use crate::config::Config;
use crate::db::{Db, GraphRecord};
use crate::error::AppError;
use crate::events::ServerEvent;
use crate::lean::{DepGraph, JobRequest, LeanEvent, LeanQueue};
use crate::model::{Assert, Fact, Rule};

#[derive(Clone)]
struct AppState {
    db: Db,
    auth: Auth,
    queue: LeanQueue,
    events: broadcast::Sender<ServerEvent>,
}

struct Authed {
    id: String,
    email: String,
    username: String,
}

impl FromRequestParts<AppState> for Authed {
    type Rejection = AppError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let header = parts
            .headers
            .get(axum::http::header::AUTHORIZATION)
            .and_then(|value| value.to_str().ok());
        let Some(token) = header.and_then(|value| value.strip_prefix("Bearer ")) else {
            return Err(AppError::unauthorized("unauthorized"));
        };
        if token.is_empty() {
            return Err(AppError::unauthorized("unauthorized"));
        }
        if let Some(person) = state.db.person_for_token(token)? {
            let username = state.db.profile_username(&person.0)?.unwrap_or_default();
            return Ok(Authed {
                id: person.0,
                email: person.1,
                username,
            });
        }
        let person = state
            .auth
            .verify(token)
            .await
            .map_err(|_| AppError::unauthorized("unauthorized"))?;
        state.db.note_email(&person.id, &person.email)?;
        let username = person.username.as_deref().filter(|name| valid_username(name));
        state
            .db
            .upsert_profile(&person.id, username, &person.email)?;
        let username = state.db.profile_username(&person.id)?.unwrap_or_default();
        Ok(Authed {
            id: person.id,
            email: person.email,
            username,
        })
    }
}

pub(super) fn valid_username(name: &str) -> bool {
    let mut count = 0;
    for ch in name.chars() {
        count += 1;
        if !(ch.is_ascii_lowercase() || ch.is_ascii_digit() || ch == '_') {
            return false;
        }
    }
    (3..=32).contains(&count)
}

#[derive(Deserialize)]
struct WorkspacePath {
    workspace_id: String,
}

async fn open_scope(parts: &mut Parts, state: &AppState) -> Result<Scope, AppError> {
    let Path(WorkspacePath { workspace_id }) = Path::from_request_parts(parts, state)
        .await
        .map_err(|_| AppError::not_found())?;
    let authed = Authed::from_request_parts(parts, state).await?;
    let Some((role, creator)) = state.db.membership(&workspace_id, &authed.id)? else {
        return Err(AppError::not_found());
    };
    Ok(Scope::new(
        workspace_id,
        authed.id,
        authed.email,
        authed.username,
        role == "editor" || creator,
        creator,
    ))
}

impl FromRequestParts<AppState> for Scope {
    type Rejection = AppError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        open_scope(parts, state).await
    }
}

impl FromRequestParts<AppState> for Editor {
    type Rejection = AppError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let scope = open_scope(parts, state).await?;
        if scope.editor() {
            Ok(Editor::new(scope))
        } else {
            Err(AppError::forbidden("you can look, but not edit this"))
        }
    }
}

impl FromRequestParts<AppState> for Creator {
    type Rejection = AppError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let scope = open_scope(parts, state).await?;
        if scope.creator() {
            Ok(Creator::new(Editor::new(scope)))
        } else {
            Err(AppError::forbidden(
                "only the person who made this workspace can change that",
            ))
        }
    }
}

pub async fn serve(config: Config) -> Result<()> {
    let database_url = config.server.database_url.clone();
    let database_ca = config.server.database_ca.clone();
    let db = std::thread::spawn(move || Db::open(&database_url, &database_ca))
        .join()
        .map_err(|_| anyhow::anyhow!("database thread failed"))??;
    let queue = LeanQueue::start(config.lean);
    let (events, _) = broadcast::channel(256);
    let state = AppState {
        db,
        auth: Auth::new(&config.auth.supabase_url),
        queue,
        events,
    };
    forward_lean(&state);
    for id in state.db.workspace_ids()? {
        enqueue_debate(&state, &id)?;
    }
    let app = router(state);
    let listener = TcpListener::bind(&config.server.bind)
        .await
        .with_context(|| format!("failed to bind {}", config.server.bind))?;
    tracing::info!(bind = %config.server.bind, "listening");
    axum::serve(listener, app)
        .await
        .context("http server stopped")?;
    Ok(())
}

fn router(state: AppState) -> Router {
    Router::new()
        .route("/me", get(live::me))
        .route("/account/token", get(live::show_token).post(live::reset_token))
        .route("/ws", get(live::ws))
        .route("/workspaces", get(workspaces::list).post(workspaces::create))
        .route(
            "/workspaces/{workspace_id}",
            put(workspaces::rename).delete(workspaces::delete),
        )
        .route("/workspaces/{workspace_id}/leave", post(workspaces::leave))
        .route(
            "/workspaces/{workspace_id}/members",
            get(workspaces::members).post(workspaces::add_member),
        )
        .route(
            "/workspaces/{workspace_id}/members/{user_id}",
            put(workspaces::set_role).delete(workspaces::remove_member),
        )
        .route(
            "/workspaces/{workspace_id}/facts/{id}/derive",
            post(facts::derive_fact),
        )
        .route(
            "/workspaces/{workspace_id}/facts",
            get(facts::list_facts).post(facts::create_fact),
        )
        .route(
            "/workspaces/{workspace_id}/facts/{id}",
            put(facts::update_fact).delete(facts::delete_fact),
        )
        .route(
            "/workspaces/{workspace_id}/rules",
            get(rules::list_rules).post(rules::create_rule),
        )
        .route(
            "/workspaces/{workspace_id}/rules/{id}",
            put(rules::update_rule).delete(rules::delete_rule),
        )
        .route(
            "/workspaces/{workspace_id}/rules/{id}/graph",
            get(rules::rule_graph),
        )
        .route(
            "/workspaces/{workspace_id}/labels",
            get(facts::list_labels).put(facts::put_labels),
        )
        .route(
            "/workspaces/{workspace_id}/labels/{id}",
            axum::routing::delete(facts::delete_label),
        )
        .route(
            "/workspaces/{workspace_id}/asserts",
            get(asserts::list_asserts).post(asserts::create_assert),
        )
        .route(
            "/workspaces/{workspace_id}/asserts/{id}",
            put(asserts::update_assert).delete(asserts::delete_assert),
        )
        .route(
            "/workspaces/{workspace_id}/asserts/{id}/graph",
            get(asserts::assert_graph),
        )
        .route(
            "/workspaces/{workspace_id}/asserts/{id}/for/{person}",
            get(people::check_person),
        )
        .route(
            "/workspaces/{workspace_id}/people",
            get(people::list_people).post(people::create_person),
        )
        .route(
            "/workspaces/{workspace_id}/people/{id}",
            axum::routing::delete(people::delete_person),
        )
        .route(
            "/workspaces/{workspace_id}/personal-facts",
            get(people::list_personal).post(people::create_personal),
        )
        .route(
            "/workspaces/{workspace_id}/personal-facts/{id}",
            axum::routing::delete(people::delete_personal),
        )
        .route(
            "/workspaces/{workspace_id}/people/{person}/toggles/{fact}",
            put(people::toggle),
        )
        .route(
            "/workspaces/{workspace_id}/comments",
            get(comments::list_comments).post(comments::create_comment),
        )
        .route(
            "/workspaces/{workspace_id}/comments/{id}",
            put(comments::update_comment).delete(comments::delete_comment),
        )
        .route(
            "/workspaces/{workspace_id}/comments/{id}/resolved",
            post(comments::resolve_comment),
        )
        .route("/workspaces/{workspace_id}/inbox", get(comments::inbox))
        .layer(CorsLayer::permissive())
        .with_state(state)
}

fn forward_lean(state: &AppState) {
    let mut rx = state.queue.subscribe();
    let db = state.db.clone();
    let events = state.events.clone();
    tokio::spawn(async move {
        loop {
            match rx.recv().await {
                Ok(event) => {
                    record_lean(&db, &event);
                    let _ = events.send(ServerEvent::from(event));
                }
                Err(broadcast::error::RecvError::Lagged(skipped)) => {
                    tracing::error!("missed {skipped} lean events");
                }
                Err(broadcast::error::RecvError::Closed) => break,
            }
        }
    });
}

fn record_lean(db: &Db, event: &LeanEvent) {
    let result = match event {
        LeanEvent::GraphUpdated {
            workspace_id,
            target_rule_id,
            graph,
            stamp,
        } => {
            let saved = if assert_known(db, workspace_id, target_rule_id) {
                db.record_assert_proved(workspace_id, target_rule_id, graph)
            } else {
                db.record_proved(workspace_id, target_rule_id, graph)
            };
            saved.and_then(|()| db.put_build_stamp(workspace_id, target_rule_id, stamp))
        }
        LeanEvent::CompileFailed {
            workspace_id,
            target_rule_id,
            diagnostics,
            stamp,
        } => {
            let saved = if assert_known(db, workspace_id, target_rule_id) {
                db.record_assert_invalid(workspace_id, target_rule_id, diagnostics)
            } else {
                db.record_invalid(workspace_id, target_rule_id, diagnostics)
            };
            match stamp {
                Some(stamp) => saved.and_then(|()| db.put_build_stamp(workspace_id, target_rule_id, stamp)),
                None => saved,
            }
        }
        LeanEvent::CompileStarted { .. } => Ok(()),
    };
    if let Err(err) = result {
        tracing::error!("{err:#}");
    }
}

fn assert_known(db: &Db, ws: &str, id: &str) -> bool {
    let ws = ws.to_string();
    let id = id.to_string();
    // The postgres client cannot block on the tokio thread.
    std::thread::scope(|scope| {
        scope
            .spawn(move || {
                db.list_asserts_in(&ws)
                    .ok()
                    .map(|asserts| asserts.iter().any(|assert| assert.id == id))
                    .unwrap_or(false)
            })
            .join()
            .unwrap_or(false)
    })
}

fn enqueue_debate(state: &AppState, ws: &str) -> Result<()> {
    let loaded = std::thread::scope(|scope| {
        scope
            .spawn(|| -> Result<_> {
                let facts = state.db.list_facts_in(ws)?;
                let rules = state.db.list_rules_in(ws)?;
                let asserts = state.db.list_asserts_in(ws)?;
                Ok((facts, rules, asserts))
            })
            .join()
            .map_err(|_| anyhow::anyhow!("database thread panicked"))?
    })?;
    let (facts, rules, asserts) = loaded;
    let mut building = 0usize;
    let mut skipped = 0usize;
    for rule in &rules {
        let line = crate::lean::stamp::rule_line(rule);
        let job = stamped_job(ws, facts.clone(), rules.clone(), rule.id.clone(), line, false);
        if fresh(state, &job)? {
            skipped += 1;
            continue;
        }
        building += 1;
        state.db.mark_compiling(ws, &rule.id)?;
        publish(
            state,
            ServerEvent::CompileStarted {
                workspace_id: ws.to_string(),
                target_rule_id: rule.id.clone(),
            },
        );
        spawn_job(state, job);
    }
    for assert in &asserts {
        let job = assert_job(ws, assert, &facts, &rules);
        if fresh(state, &job)? {
            skipped += 1;
            continue;
        }
        building += 1;
        state.db.mark_assert_compiling(ws, &assert.id)?;
        publish(
            state,
            ServerEvent::CompileStarted {
                workspace_id: ws.to_string(),
                target_rule_id: assert.id.clone(),
            },
        );
        spawn_job(state, job);
    }
    tracing::info!(skipped, building, "lean queue");
    Ok(())
}

fn spawn_job(state: &AppState, job: JobRequest) {
    let queue = state.queue.clone();
    tokio::spawn(async move {
        if let Err(err) = queue.submit(job).await {
            tracing::error!("{err:#}");
        }
    });
}

fn fresh(state: &AppState, job: &JobRequest) -> Result<bool> {
    let Some(saved) = state.db.build_stamp(&job.workspace_id, &job.target_rule_id)? else {
        return Ok(false);
    };
    let Some(record) = state.db.saved_graph(&job.workspace_id, &job.target_rule_id)? else {
        return Ok(false);
    };
    let fresh = match record.status.as_str() {
        "proved" => record
            .graph
            .as_ref()
            .and_then(|graph| {
                crate::lean::stamp::proved_stamp(
                    graph,
                    &job.facts,
                    &job.rules,
                    &job.target_rule_id,
                    &job.target_line,
                )
            })
            .unwrap_or_else(|| crate::lean::stamp::library_stamp(&job.facts, &job.rules, &job.target_line)),
        "invalid" => crate::lean::stamp::library_stamp(&job.facts, &job.rules, &job.target_line),
        _ => return Ok(false),
    };
    Ok(saved == fresh)
}

fn stamped_job(
    ws: &str,
    facts: Vec<Fact>,
    rules: Vec<Rule>,
    target_rule_id: String,
    target_line: String,
    ephemeral: bool,
) -> JobRequest {
    let stamp = crate::lean::stamp::library_stamp(&facts, &rules, &target_line);
    JobRequest {
        workspace_id: ws.to_string(),
        facts,
        rules,
        target_rule_id,
        ephemeral,
        target_line,
        stamp,
    }
}

fn assert_job(ws: &str, assert: &Assert, facts: &[Fact], rules: &[Rule]) -> JobRequest {
    let mut facts = facts.to_vec();
    for id in &assert.assumes {
        if let Some(fact) = facts.iter_mut().find(|fact| fact.id == *id) {
            fact.role = "fact".into();
        }
    }
    let mut premises: Vec<String> = facts
        .iter()
        .filter(|fact| fact.role != "criterion")
        .map(|fact| fact.id.clone())
        .collect();
    premises.extend(rules.iter().map(|rule| rule.id.clone()));
    let mut rules = rules.to_vec();
    rules.push(Rule {
        id: assert.id.clone(),
        premises,
        conclusion: assert.formula.clone(),
    });
    let target_line = crate::lean::stamp::assert_line(&assert.id, &assert.formula, &assert.assumes);
    stamped_job(ws, facts, rules, assert.id.clone(), target_line, false)
}

fn publish(state: &AppState, event: ServerEvent) {
    let _ = state.events.send(event);
}

#[derive(Serialize)]
struct GraphBody {
    status: String,
    graph: Option<DepGraph>,
    diagnostics: Option<String>,
}

fn graph_body(record: Option<GraphRecord>) -> GraphBody {
    match record {
        Some(record) => GraphBody {
            status: record.status,
            graph: record.graph,
            diagnostics: record.diagnostics,
        },
        None => GraphBody {
            status: "pending".into(),
            graph: None,
            diagnostics: None,
        },
    }
}

fn id_taken(state: &AppState, scope: &Scope, id: &str) -> Result<bool, AppError> {
    Ok(state.db.fact(scope, id)?.is_some()
        || state.db.rule(scope, id)?.is_some()
        || state.db.get_assert(scope, id)?.is_some())
}

#[derive(Deserialize)]
pub(super) struct IdPath {
    pub id: String,
}

fn require_id(id: &str) -> Result<(), AppError> {
    if id.is_empty() || id.chars().any(char::is_whitespace) {
        return Err(AppError::bad_request("id must be a non-empty token without spaces"));
    }
    Ok(())
}

fn clean_text(value: &str, label: &str) -> Result<String, AppError> {
    let value = value.trim();
    if value.is_empty() {
        return Err(AppError::bad_request(format!("{label} is required")));
    }
    Ok(value.to_string())
}

fn clean_citations(raw: Vec<String>) -> Vec<String> {
    raw.into_iter()
        .map(|citation| citation.trim().to_string())
        .filter(|citation| !citation.is_empty())
        .collect()
}

fn blank_to_none(value: Option<String>) -> Option<String> {
    value.and_then(|text| {
        let text = text.trim().to_string();
        if text.is_empty() {
            None
        } else {
            Some(text)
        }
    })
}
