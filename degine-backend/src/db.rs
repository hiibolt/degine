use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};

use anyhow::{Context, Result};
use postgres::{Client, Row, Transaction};
use rustls::ClientConfig;
use rustls::RootCertStore;
use tokio_postgres_rustls::MakeRustlsConnect;

use crate::lean::DepGraph;
use crate::model::{Assert, Comment, Fact, Person, PersonalFact, Rule};

#[derive(Clone)]
pub struct Db {
    slots: Arc<Vec<Mutex<Client>>>,
    next: Arc<AtomicUsize>,
    owner: Arc<Mutex<Option<String>>>,
    emails: Arc<Mutex<HashMap<String, String>>>,
}

pub struct GraphRecord {
    pub status: String,
    pub graph: Option<DepGraph>,
    pub diagnostics: Option<String>,
}

impl Db {
    pub fn open(url: &str, ca_file: &str) -> Result<Self> {
        let _ = rustls::crypto::ring::default_provider().install_default();
        let mut roots = RootCertStore::empty();
        roots.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
        if !ca_file.is_empty() {
            let mut reader = std::io::BufReader::new(
                std::fs::File::open(ca_file)
                    .with_context(|| format!("failed to open the database ca {ca_file}"))?,
            );
            for cert in rustls_pemfile::certs(&mut reader) {
                let cert = cert.context("the database ca was unreadable")?;
                roots
                    .add(cert)
                    .context("the database ca could not be trusted")?;
            }
        }
        let config = ClientConfig::builder()
            .with_root_certificates(roots)
            .with_no_client_auth();
        let connector = MakeRustlsConnect::new(config);
        let mut slots = Vec::new();
        for _ in 0..4 {
            slots.push(Mutex::new(
                Client::connect(url, connector.clone()).context("failed to open the database")?,
            ));
        }
        let db = Self {
            slots: Arc::new(slots),
            next: Arc::new(AtomicUsize::new(0)),
            owner: Arc::new(Mutex::new(None)),
            emails: Arc::new(Mutex::new(HashMap::new())),
        };
        Ok(db)
    }

    fn cached_owner(&self) -> Option<String> {
        self.owner.lock().ok().and_then(|guard| guard.clone())
    }

    fn remember_owner(&self, user_id: &str) {
        if let Ok(mut guard) = self.owner.lock() {
            *guard = Some(user_id.to_string());
        }
    }

    fn hop<T: Send>(&self, f: impl FnOnce(&Db) -> Result<T> + Send) -> Result<T> {
        let db = self.clone();
        std::thread::scope(|scope| {
            scope
                .spawn(move || f(&db))
                .join()
                .map_err(|_| anyhow::anyhow!("database thread panicked"))?
        })
    }

    fn lock(&self) -> Result<MutexGuard<'_, Client>> {
        let index = self.next.fetch_add(1, Ordering::Relaxed) % self.slots.len();
        self.slots[index]
            .lock()
            .map_err(|_| anyhow::anyhow!("database lock poisoned"))
    }

    pub fn list_facts(&self) -> Result<Vec<Fact>> {
        self.hop(|db| {
            db.list_facts_inner().context("failed to list facts")
        })
    }

    fn list_facts_inner(&self) -> Result<Vec<Fact>> {
        let mut conn = self.lock()?;
        let rows = conn.query(
            "SELECT id, claim, formula, role FROM facts ORDER BY id",
            &[],
        )?;
        let mut facts: Vec<Fact> = rows.iter().map(read_fact).collect::<Result<_, _>>()?;
        attach_citations(&mut conn, &mut facts)?;
        Ok(facts)
    }

    pub fn fact(&self, id: &str) -> Result<Option<Fact>> {
        self.hop(|db| {
            db.fact_inner(id)
                .with_context(|| format!("failed to read fact `{id}`"))
        })
    }

    fn fact_inner(&self, id: &str) -> Result<Option<Fact>> {
        let mut conn = self.lock()?;
        let found = conn.query_opt(
            "SELECT id, claim, formula, role FROM facts WHERE id = $1",
            &[&id],
        )?;
        let Some(row) = found else {
            return Ok(None);
        };
        let mut fact = read_fact(&row)?;
        fact.citations = citations_for(&mut conn, id)?;
        Ok(Some(fact))
    }

    pub fn insert_fact(&self, fact: &Fact) -> Result<()> {
        self.hop(|db| {
            db.insert_fact_inner(fact)
                .with_context(|| format!("failed to insert fact `{}`", fact.id))
        })
    }

    fn insert_fact_inner(&self, fact: &Fact) -> Result<()> {
        let mut conn = self.lock()?;
        let mut tx = conn.transaction()?;
        tx.execute(
            "INSERT INTO facts (id, claim, formula, role) VALUES ($1, $2, $3, $4)",
            &[&fact.id, &fact.claim, &fact.formula, &fact.role],
        )?;
        write_citations(&mut tx, &fact.id, &fact.citations)?;
        tx.commit()?;
        Ok(())
    }

    pub fn update_fact(&self, fact: &Fact) -> Result<bool> {
        self.hop(|db| {
            db.update_fact_inner(fact)
                .with_context(|| format!("failed to update fact `{}`", fact.id))
        })
    }

    fn update_fact_inner(&self, fact: &Fact) -> Result<bool> {
        let mut conn = self.lock()?;
        let mut tx = conn.transaction()?;
        let changed = tx.execute(
            "UPDATE facts SET claim = $2, formula = $3, role = $4 WHERE id = $1",
            &[&fact.id, &fact.claim, &fact.formula, &fact.role],
        )?;
        if changed == 0 {
            return Ok(false);
        }
        tx.execute("DELETE FROM fact_citations WHERE fact_id = $1", &[&fact.id])?;
        write_citations(&mut tx, &fact.id, &fact.citations)?;
        tx.commit()?;
        Ok(true)
    }

    pub fn delete_fact(&self, id: &str) -> Result<bool> {
        self.hop(|_db| {
            let changed = self
                .lock()?
                .execute("DELETE FROM facts WHERE id = $1", &[&id])
                .with_context(|| format!("failed to delete fact `{id}`"))?;
            Ok(changed > 0)
        })
    }

    pub fn list_rules(&self) -> Result<Vec<Rule>> {
        self.hop(|db| {
            db.list_rules_inner().context("failed to list rules")
        })
    }

    fn list_rules_inner(&self) -> Result<Vec<Rule>> {
        let mut conn = self.lock()?;
        let rows = conn.query("SELECT id, conclusion FROM rules ORDER BY id", &[])?;
        let mut rules: Vec<Rule> = rows.iter().map(read_rule).collect::<Result<_, _>>()?;
        attach_premises(&mut conn, &mut rules)?;
        Ok(rules)
    }

    pub fn rule(&self, id: &str) -> Result<Option<Rule>> {
        self.hop(|db| {
            db.rule_inner(id)
                .with_context(|| format!("failed to read rule `{id}`"))
        })
    }

    fn rule_inner(&self, id: &str) -> Result<Option<Rule>> {
        let mut conn = self.lock()?;
        let found = conn.query_opt(
            "SELECT id, conclusion FROM rules WHERE id = $1",
            &[&id],
        )?;
        let Some(row) = found else {
            return Ok(None);
        };
        let mut rule = read_rule(&row)?;
        rule.premises = premises_for(&mut conn, id)?;
        Ok(Some(rule))
    }

    pub fn insert_rule(&self, rule: &Rule) -> Result<()> {
        self.hop(|db| {
            db.insert_rule_inner(rule)
                .with_context(|| format!("failed to insert rule `{}`", rule.id))
        })
    }

    fn insert_rule_inner(&self, rule: &Rule) -> Result<()> {
        let mut conn = self.lock()?;
        let mut tx = conn.transaction()?;
        tx.execute(
            "INSERT INTO rules (id, conclusion) VALUES ($1, $2)",
            &[&rule.id, &rule.conclusion],
        )?;
        write_premises(&mut tx, &rule.id, &rule.premises)?;
        tx.execute(
            "INSERT INTO graphs (rule_id, status) VALUES ($1, 'pending')",
            &[&rule.id],
        )?;
        tx.commit()?;
        Ok(())
    }

    pub fn update_rule(&self, rule: &Rule) -> Result<bool> {
        self.hop(|db| {
            db.update_rule_inner(rule)
                .with_context(|| format!("failed to update rule `{}`", rule.id))
        })
    }

    fn update_rule_inner(&self, rule: &Rule) -> Result<bool> {
        let mut conn = self.lock()?;
        let mut tx = conn.transaction()?;
        let changed = tx.execute(
            "UPDATE rules SET conclusion = $2 WHERE id = $1",
            &[&rule.id, &rule.conclusion],
        )?;
        if changed == 0 {
            return Ok(false);
        }
        tx.execute("DELETE FROM rule_premises WHERE rule_id = $1", &[&rule.id])?;
        write_premises(&mut tx, &rule.id, &rule.premises)?;
        tx.commit()?;
        Ok(true)
    }

    pub fn list_labels(&self) -> Result<std::collections::BTreeMap<String, String>> {
        self.hop(|db| {
            let mut conn = db.lock()?;
            let rows = conn.query(
                "SELECT id, title FROM labels WHERE id NOT LIKE '~%' ORDER BY id",
                &[],
            )?;
            let mut labels = std::collections::BTreeMap::new();
            for row in rows {
                labels.insert(row.get(0), row.get(1));
            }
            Ok(labels)
        })
    }

    pub fn delete_label(&self, id: &str) -> Result<Option<()>> {
        self.hop(|db| {
            let mut conn = db.lock()?;
            let found = conn
                .query_opt("SELECT id FROM labels WHERE id = $1", &[&id])
                .with_context(|| format!("failed to read label `{id}`"))?;
            if found.is_none() {
                return Ok(None);
            }
            if label_in_use(&mut conn, id)? {
                anyhow::bail!("in use");
            }
            conn.execute("DELETE FROM labels WHERE id = $1", &[&id])
                .with_context(|| format!("failed to delete label `{id}`"))?;
            Ok(Some(()))
        })
    }

    fn put_label(conn: &mut Client, id: &str, title: &str) -> Result<()> {
        conn.execute(
            "INSERT INTO labels (id, title) VALUES ($1, $2)
             ON CONFLICT (id) DO UPDATE SET title = EXCLUDED.title",
            &[&id, &title],
        )
        .with_context(|| format!("failed to write label `{id}`"))?;
        Ok(())
    }

    pub fn list_people(&self) -> Result<Vec<Person>> {
        self.hop(|db| {
            let mut conn = db.lock()?;
            let rows = conn.query(
                "SELECT id, title FROM labels WHERE id LIKE '~person:%' ORDER BY title",
                &[],
            )?;
            let toggles = conn.query("SELECT id FROM labels WHERE id LIKE '~on:%'", &[])?;
            let mut on: HashMap<String, Vec<String>> = HashMap::new();
            for row in toggles {
                let id: String = row.get(0);
                let mut parts = id.splitn(3, ':');
                let _ = parts.next();
                let Some(person) = parts.next() else { continue };
                let Some(fact) = parts.next() else { continue };
                on.entry(person.to_string()).or_default().push(fact.to_string());
            }
            Ok(rows
                .into_iter()
                .map(|row| {
                    let raw: String = row.get(0);
                    let id = raw.trim_start_matches("~person:").to_string();
                    Person {
                        on: on.remove(&id).unwrap_or_default(),
                        id,
                        name: row.get(1),
                    }
                })
                .collect())
        })
    }

    pub fn insert_person(&self, id: &str, name: &str) -> Result<()> {
        self.hop(|db| {
            let mut conn = db.lock()?;
            Self::put_label(&mut conn, &format!("~person:{id}"), name)
        })
    }

    pub fn delete_person(&self, id: &str) -> Result<bool> {
        self.hop(|db| {
            let mut conn = db.lock()?;
            let n = conn
                .execute("DELETE FROM labels WHERE id = $1 OR id LIKE $2", &[
                    &format!("~person:{id}"),
                    &format!("~on:{id}:%"),
                ])
                .context("failed to delete a person")?;
            Ok(n > 0)
        })
    }

    pub fn list_personal_facts(&self) -> Result<Vec<PersonalFact>> {
        self.hop(|db| {
            let mut conn = db.lock()?;
            let rows = conn.query(
                "SELECT id, title FROM labels WHERE id LIKE '~pfact:%' ORDER BY title",
                &[],
            )?;
            Ok(rows
                .into_iter()
                .map(|row| {
                    let raw: String = row.get(0);
                    PersonalFact {
                        id: raw.trim_start_matches("~pfact:").to_string(),
                        claim: row.get(1),
                    }
                })
                .collect())
        })
    }

    pub fn insert_personal_fact(&self, id: &str, claim: &str) -> Result<()> {
        self.hop(|db| {
            let mut conn = db.lock()?;
            Self::put_label(&mut conn, &format!("~pfact:{id}"), claim)
        })
    }

    pub fn delete_personal_fact(&self, id: &str) -> Result<bool> {
        self.hop(|db| {
            let mut conn = db.lock()?;
            let n = conn
                .execute("DELETE FROM labels WHERE id = $1 OR id LIKE $2", &[
                    &format!("~pfact:{id}"),
                    &format!("~on:%:{id}"),
                ])
                .context("failed to delete a personal fact")?;
            Ok(n > 0)
        })
    }

    pub fn set_toggle(&self, person: &str, fact: &str, on: bool) -> Result<()> {
        self.hop(|db| {
            let mut conn = db.lock()?;
            let id = format!("~on:{person}:{fact}");
            if on {
                Self::put_label(&mut conn, &id, "on")?;
            } else {
                conn.execute("DELETE FROM labels WHERE id = $1", &[&id])
                    .context("failed to set a personal fact")?;
            }
            Ok(())
        })
    }

    pub fn upsert_labels(&self, labels: &std::collections::BTreeMap<String, String>) -> Result<()> {
        self.hop(|db| {
            let mut conn = db.lock()?;
            for (id, title) in labels {
                let title = title.trim();
                if id.is_empty() || title.is_empty() {
                    continue;
                }
                conn.execute(
                    "INSERT INTO labels (id, title) VALUES ($1, $2)
                     ON CONFLICT(id) DO UPDATE SET title = excluded.title",
                    &[&id, &title],
                )?;
            }
            Ok(())
        })
    }

    pub fn list_asserts(&self) -> Result<Vec<Assert>> {
        self.hop(|db| {
            db.list_asserts_inner().context("failed to list asserts")
        })
    }

    fn list_asserts_inner(&self) -> Result<Vec<Assert>> {
        let mut conn = self.lock()?;
        let rows = conn.query(
            "SELECT id, title, description, formula FROM asserts ORDER BY id",
            &[],
        )?;
        rows.iter().map(read_assert).collect::<Result<_, _>>()
    }

    pub fn get_assert(&self, id: &str) -> Result<Option<Assert>> {
        self.hop(|db| {
            db.get_assert_inner(id)
                .with_context(|| format!("failed to read assert `{id}`"))
        })
    }

    fn get_assert_inner(&self, id: &str) -> Result<Option<Assert>> {
        let mut conn = self.lock()?;
        let found = conn.query_opt(
            "SELECT id, title, description, formula FROM asserts WHERE id = $1",
            &[&id],
        )?;
        found.map(|row| read_assert(&row)).transpose()
    }

    pub fn insert_assert(&self, assert: &Assert) -> Result<()> {
        self.hop(|db| {
            db.insert_assert_inner(assert)
                .with_context(|| format!("failed to insert assert `{}`", assert.id))
        })
    }

    fn insert_assert_inner(&self, assert: &Assert) -> Result<()> {
        let mut conn = self.lock()?;
        let mut tx = conn.transaction()?;
        tx.execute(
            "INSERT INTO asserts (id, title, description, formula) VALUES ($1, $2, $3, $4)",
            &[
                &assert.id,
                &assert.title,
                &assert.description,
                &assert.formula,
            ],
        )?;
        tx.execute(
            "INSERT INTO assert_graphs (assert_id, status) VALUES ($1, 'pending')",
            &[&assert.id],
        )?;
        tx.commit()?;
        Ok(())
    }

    pub fn update_assert(&self, assert: &Assert) -> Result<bool> {
        self.hop(|db| {
            db.update_assert_inner(assert)
                .with_context(|| format!("failed to update assert `{}`", assert.id))
        })
    }

    fn update_assert_inner(&self, assert: &Assert) -> Result<bool> {
        let changed = self.lock()?.execute(
            "UPDATE asserts SET title = $2, description = $3, formula = $4 WHERE id = $1",
            &[
                &assert.id,
                &assert.title,
                &assert.description,
                &assert.formula,
            ],
        )?;
        Ok(changed > 0)
    }

    pub fn delete_assert(&self, id: &str) -> Result<bool> {
        self.hop(|_db| {
            let changed = self
                .lock()?
                .execute("DELETE FROM asserts WHERE id = $1", &[&id])
                .with_context(|| format!("failed to delete assert `{id}`"))?;
            Ok(changed > 0)
        })
    }

    pub fn mark_assert_compiling(&self, id: &str) -> Result<()> {
        self.hop(|db| {
            db.lock()?
                .execute(
                    "UPDATE assert_graphs SET status = 'pending' WHERE assert_id = $1",
                    &[&id],
                )
                .with_context(|| format!("failed to mark assert `{id}` as compiling"))?;
            Ok(())
        })
    }

    pub fn record_assert_proved(&self, id: &str, graph: &DepGraph) -> Result<()> {
        self.hop(|db| {
            let json = serde_json::to_string(graph).context("failed to encode the dependency graph")?;
            db.lock()?
                .execute(
                    "UPDATE assert_graphs
                     SET status = 'proved', graph_json = $2, diagnostics = NULL
                     WHERE assert_id = $1",
                    &[&id, &json],
                )
                .with_context(|| format!("failed to store the graph for assert `{id}`"))?;
            Ok(())
        })
    }

    pub fn record_assert_invalid(&self, id: &str, diagnostics: &str) -> Result<()> {
        self.hop(|db| {
            db.lock()?
                .execute(
                    "UPDATE assert_graphs
                     SET status = 'invalid', graph_json = NULL, diagnostics = $2
                     WHERE assert_id = $1",
                    &[&id, &diagnostics],
                )
                .with_context(|| format!("failed to store diagnostics for assert `{id}`"))?;
            Ok(())
        })
    }

    pub fn assert_graph(&self, id: &str) -> Result<Option<GraphRecord>> {
        self.hop(|db| {
            db.assert_graph_inner(id)
                .with_context(|| format!("failed to read the graph for assert `{id}`"))
        })
    }

    fn assert_graph_inner(&self, id: &str) -> Result<Option<GraphRecord>> {
        let mut conn = self.lock()?;
        let found = conn.query_opt(
            "SELECT status, graph_json, diagnostics FROM assert_graphs WHERE assert_id = $1",
            &[&id],
        )?;
        graph_record(found)
    }

    pub fn delete_rule(&self, id: &str) -> Result<bool> {
        self.hop(|_db| {
            let changed = self
                .lock()?
                .execute("DELETE FROM rules WHERE id = $1", &[&id])
                .with_context(|| format!("failed to delete rule `{id}`"))?;
            Ok(changed > 0)
        })
    }

    pub fn list_comments(&self, target_type: &str, target_id: &str) -> Result<Vec<Comment>> {
        self.hop(|db| {
            db.list_comments_inner(target_type, target_id)
                .context("failed to list comments")
        })
    }

    fn list_comments_inner(&self, target_type: &str, target_id: &str) -> Result<Vec<Comment>> {
        let mut conn = self.lock()?;
        let rows = conn.query(
            "SELECT id, target_type, target_id, author, body, created_at, resolved
             FROM comments
             WHERE target_type = $1 AND target_id = $2
             ORDER BY id",
            &[&target_type, &target_id],
        )?;
        rows.iter().map(read_comment).collect()
    }

    pub fn insert_comment(
        &self,
        target_type: &str,
        target_id: &str,
        author: &str,
        body: &str,
    ) -> Result<Comment> {
        self.hop(|db| {
            db.insert_comment_inner(target_type, target_id, author, body)
                .context("failed to insert comment")
        })
    }

    fn insert_comment_inner(
        &self,
        target_type: &str,
        target_id: &str,
        author: &str,
        body: &str,
    ) -> Result<Comment> {
        let created_at = chrono::Utc::now().to_rfc3339();
        let mut conn = self.lock()?;
        let id: i64 = conn.query_one(
            "INSERT INTO comments (target_type, target_id, author, body, created_at)
             VALUES ($1, $2, $3, $4, $5)
             RETURNING id",
            &[&target_type, &target_id, &author, &body, &created_at],
        )?.get(0);
        Ok(Comment {
            id,
            target_type: target_type.to_string(),
            target_id: target_id.to_string(),
            author: author.to_string(),
            body: body.to_string(),
            created_at,
            resolved: false,
        })
    }

    pub fn comment(&self, id: i64) -> Result<Option<Comment>> {
        self.hop(|db| {
            db.comment_inner(id)
                .with_context(|| format!("failed to read comment {id}"))
        })
    }

    fn comment_inner(&self, id: i64) -> Result<Option<Comment>> {
        let mut conn = self.lock()?;
        let found = conn.query_opt(
            "SELECT id, target_type, target_id, author, body, created_at, resolved
             FROM comments WHERE id = $1",
            &[&id],
        )?;
        found.map(|row| read_comment(&row)).transpose()
    }

    pub fn update_comment(&self, id: i64, body: &str) -> Result<Option<Comment>> {
        self.hop(|db| {
            db.update_comment_inner(id, body)
                .with_context(|| format!("failed to update comment {id}"))
        })
    }

    fn update_comment_inner(&self, id: i64, body: &str) -> Result<Option<Comment>> {
        let changed = self
            .lock()?
            .execute("UPDATE comments SET body = $2 WHERE id = $1", &[&id, &body])?;
        if changed == 0 {
            return Ok(None);
        }
        self.comment_inner(id)
    }

    pub fn set_comment_resolved(&self, id: i64, resolved: bool) -> Result<Option<Comment>> {
        self.hop(|db| {
            db.set_comment_resolved_inner(id, resolved)
                .with_context(|| format!("failed to resolve comment {id}"))
        })
    }

    fn set_comment_resolved_inner(&self, id: i64, resolved: bool) -> Result<Option<Comment>> {
        let changed = self.lock()?.execute(
            "UPDATE comments SET resolved = $2 WHERE id = $1",
            &[&id, &resolved],
        )?;
        if changed == 0 {
            return Ok(None);
        }
        self.comment_inner(id)
    }

    pub fn delete_comment(&self, id: i64) -> Result<bool> {
        self.hop(|_db| {
            let changed = self
                .lock()?
                .execute("DELETE FROM comments WHERE id = $1", &[&id])
                .with_context(|| format!("failed to delete comment {id}"))?;
            Ok(changed > 0)
        })
    }

    pub fn list_open_comments(&self) -> Result<Vec<Comment>> {
        self.hop(|db| {
            let mut conn = db.lock()?;
            let rows = conn.query(
                "SELECT id, target_type, target_id, author, body, created_at, resolved
                 FROM comments WHERE resolved = false ORDER BY id DESC",
                &[],
            )?;
            rows.iter().map(read_comment).collect()
        })
    }

    pub fn shares_for(&self, email: &str) -> Result<Vec<String>> {
        self.hop(|db| {
            let mut conn = db.lock()?;
            let rows = conn.query("SELECT assert_id FROM shares WHERE email = $1", &[&email])?;
            Ok(rows.iter().map(|row| row.get(0)).collect())
        })
    }

    pub fn shares_of(&self, assert_id: &str) -> Result<Vec<String>> {
        self.hop(|db| {
            let mut conn = db.lock()?;
            let rows = conn.query(
                "SELECT email FROM shares WHERE assert_id = $1 ORDER BY email",
                &[&assert_id],
            )?;
            Ok(rows.iter().map(|row| row.get(0)).collect())
        })
    }

    pub fn grant_share(&self, assert_id: &str, email: &str) -> Result<()> {
        self.hop(|db| {
            db.lock()?
                .execute(
                    "INSERT INTO shares (assert_id, email) VALUES ($1, $2) ON CONFLICT DO NOTHING",
                    &[&assert_id, &email],
                )
                .with_context(|| format!("failed to share assert `{assert_id}`"))?;
            Ok(())
        })
    }

    pub fn revoke_share(&self, assert_id: &str, email: &str) -> Result<bool> {
        self.hop(|_db| {
            let changed = self
                .lock()?
                .execute(
                    "DELETE FROM shares WHERE assert_id = $1 AND email = $2",
                    &[&assert_id, &email],
                )
                .with_context(|| format!("failed to unshare assert `{assert_id}`"))?;
            Ok(changed > 0)
        })
    }

    pub fn mark_compiling(&self, rule_id: &str) -> Result<()> {
        self.hop(|db| {
            db.lock()?
                .execute(
                    "UPDATE graphs SET status = 'pending' WHERE rule_id = $1",
                    &[&rule_id],
                )
                .with_context(|| format!("failed to mark rule `{rule_id}` as compiling"))?;
            Ok(())
        })
    }

    pub fn record_proved(&self, rule_id: &str, graph: &DepGraph) -> Result<()> {
        self.hop(|db| {
            let json = serde_json::to_string(graph).context("failed to encode the dependency graph")?;
            db.lock()?
                .execute(
                    "UPDATE graphs
                     SET status = 'proved', graph_json = $2, diagnostics = NULL
                     WHERE rule_id = $1",
                    &[&rule_id, &json],
                )
                .with_context(|| format!("failed to store the graph for `{rule_id}`"))?;
            Ok(())
        })
    }

    pub fn record_invalid(&self, rule_id: &str, diagnostics: &str) -> Result<()> {
        self.hop(|db| {
            db.lock()?
                .execute(
                    "UPDATE graphs
                     SET status = 'invalid', graph_json = NULL, diagnostics = $2
                     WHERE rule_id = $1",
                    &[&rule_id, &diagnostics],
                )
                .with_context(|| format!("failed to store diagnostics for `{rule_id}`"))?;
            Ok(())
        })
    }

    pub fn graph(&self, rule_id: &str) -> Result<Option<GraphRecord>> {
        self.hop(|db| {
            db.graph_inner(rule_id)
                .with_context(|| format!("failed to read the graph for `{rule_id}`"))
        })
    }

    fn graph_inner(&self, rule_id: &str) -> Result<Option<GraphRecord>> {
        let mut conn = self.lock()?;
        let found = conn.query_opt(
            "SELECT status, graph_json, diagnostics FROM graphs WHERE rule_id = $1",
            &[&rule_id],
        )?;
        graph_record(found)
    }

    fn setting(&self, key: &str) -> Result<Option<String>> {
        self.hop(|db| {
            let mut conn = db.lock()?;
            Ok(conn
                .query_opt("SELECT value FROM settings WHERE key = $1", &[&key])?
                .map(|row| row.get(0)))
        })
    }

    fn put_setting(&self, key: &str, value: &str) -> Result<()> {
        self.hop(|db| {
            let mut conn = db.lock()?;
            conn.execute(
                "INSERT INTO settings (key, value) VALUES ($1, $2)
                 ON CONFLICT(key) DO UPDATE SET value = excluded.value",
                &[&key, &value],
            )?;
            Ok(())
        })
    }

    pub fn note_email(&self, user_id: &str, email: &str) -> Result<()> {
        if let Ok(guard) = self.emails.lock() {
            if guard.get(user_id).map(String::as_str) == Some(email) {
                return Ok(());
            }
        }
        self.put_setting(&format!("email:{user_id}"), email)?;
        if let Ok(mut guard) = self.emails.lock() {
            guard.insert(user_id.to_string(), email.to_string());
        }
        Ok(())
    }

    pub fn api_token(&self, user_id: &str) -> Result<String> {
        let key = format!("token:{user_id}");
        if let Some(token) = self.setting(&key)? {
            return Ok(token);
        }
        let token = mint_token(user_id);
        self.put_setting(&key, &token)?;
        Ok(token)
    }

    pub fn reset_api_token(&self, user_id: &str) -> Result<String> {
        let token = mint_token(user_id);
        self.put_setting(&format!("token:{user_id}"), &token)?;
        Ok(token)
    }

    pub fn person_for_token(&self, token: &str) -> Result<Option<(String, String)>> {
        let Some(rest) = token.strip_prefix("dg1.") else {
            return Ok(None);
        };
        let Some((user_id, _)) = rest.split_once('.') else {
            return Ok(None);
        };
        let Some(stored) = self.setting(&format!("token:{user_id}"))? else {
            return Ok(None);
        };
        if stored != token {
            return Ok(None);
        }
        let Some(email) = self.setting(&format!("email:{user_id}"))? else {
            return Ok(None);
        };
        Ok(Some((user_id.to_string(), email)))
    }

    /// The fact's id stays the conclusion atom. Its text and citations move onto a new theorem.
    pub fn derive_fact(&self, old_id: &str, new_id: &str, new_claim: &str) -> Result<Fact> {
        self.hop(|db| {
            let mut conn = db.lock()?;
            let mut tx = conn.transaction()?;
            let found = tx.query_opt(
                "SELECT claim, role FROM facts WHERE id = $1",
                &[&old_id],
            )?;
            let Some(row) = found else {
                anyhow::bail!("missing");
            };
            let old_claim: String = row.get(0);
            let role: String = row.get(1);
            if role != "fact" {
                anyhow::bail!("not a fact");
            }
            if tx
                .query_opt("SELECT id FROM facts WHERE id = $1", &[&new_id])?
                .is_some()
            {
                anyhow::bail!("taken");
            }
            let citations: Vec<String> = tx
                .query(
                    "SELECT reference FROM fact_citations WHERE fact_id = $1 ORDER BY position",
                    &[&old_id],
                )?
                .iter()
                .map(|row| row.get(0))
                .collect();
            tx.execute(
                "INSERT INTO facts (id, claim, formula, role) VALUES ($1, $2, NULL, 'fact')",
                &[&new_id, &new_claim],
            )?;
            tx.execute("DELETE FROM fact_citations WHERE fact_id = $1", &[&old_id])?;
            tx.execute("DELETE FROM facts WHERE id = $1", &[&old_id])?;
            tx.execute(
                "INSERT INTO labels (id, title) VALUES ($1, $2)
                 ON CONFLICT(id) DO UPDATE SET title = excluded.title",
                &[&old_id, &old_claim],
            )?;
            let theorem_id = fresh_id(&mut tx, &format!("{old_id}_because"))?;
            let formula = format!("imp(fact:{new_id}, fact:{old_id})");
            tx.execute(
                "INSERT INTO facts (id, claim, formula, role) VALUES ($1, $2, $3, 'theorem')",
                &[&theorem_id, &old_claim, &formula],
            )?;
            write_citations(&mut tx, &theorem_id, &citations)?;
            tx.commit()?;
            Ok(Fact {
                id: theorem_id,
                claim: old_claim,
                citations,
                formula: Some(formula),
                role: "theorem".into(),
            })
        })
    }

    pub fn claim_owner(&self, user_id: &str) -> Result<()> {
        if self.cached_owner().as_deref() == Some(user_id) {
            return Ok(());
        }
        {
            let gate = self
                .owner
                .lock()
                .map_err(|_| anyhow::anyhow!("database lock poisoned"))?;
            if gate.as_deref() == Some(user_id) {
                return Ok(());
            }
        }
        self.hop(|db| {
            let mut conn = db.lock()?;
            conn.execute(
                "INSERT INTO settings (key, value) VALUES ('owner_user_id', $1) ON CONFLICT(key) DO NOTHING",
                &[&user_id],
            )
            .context("failed to record the library owner")?;
            Ok(())
        })?;
        self.remember_owner(user_id);
        Ok(())
    }

    pub fn is_owner_id(&self, user_id: &str) -> Result<bool> {
        if let Some(owner) = self.cached_owner() {
            return Ok(owner == user_id);
        }
        let stored = self.hop(|db| {
            let mut conn = db.lock()?;
            let stored: Option<String> = conn
                .query_opt(
                    "SELECT value FROM settings WHERE key = 'owner_user_id'",
                    &[],
                )?
                .map(|row| row.get(0));
            Ok(stored)
        })?;
        if let Some(owner) = &stored {
            self.remember_owner(owner);
        }
        Ok(stored.as_deref() == Some(user_id))
    }
}

fn graph_record(found: Option<Row>) -> Result<Option<GraphRecord>> {
    let Some(row) = found else {
        return Ok(None);
    };
    let status: String = row.get(0);
    let graph_json: Option<String> = row.get(1);
    let diagnostics: Option<String> = row.get(2);
    let graph = match graph_json {
        Some(text) => Some(serde_json::from_str(&text).context("stored graph is invalid json")?),
        None => None,
    };
    Ok(Some(GraphRecord {
        status,
        graph,
        diagnostics,
    }))
}

fn read_assert(row: &Row) -> Result<Assert> {
    Ok(Assert {
        id: row.get(0),
        title: row.get(1),
        description: row.get(2),
        formula: row.get(3),
    })
}

fn read_fact(row: &Row) -> Result<Fact> {
    Ok(Fact {
        id: row.get(0),
        claim: row.get(1),
        citations: Vec::new(),
        formula: row.get(2),
        role: row.get(3),
    })
}

fn read_rule(row: &Row) -> Result<Rule> {
    Ok(Rule {
        id: row.get(0),
        premises: Vec::new(),
        conclusion: row.get(1),
    })
}

fn read_comment(row: &Row) -> Result<Comment> {
    Ok(Comment {
        id: row.get(0),
        target_type: row.get(1),
        target_id: row.get(2),
        author: row.get(3),
        body: row.get(4),
        created_at: row.get(5),
        resolved: row.get(6),
    })
}

fn citations_for(conn: &mut Client, fact_id: &str) -> Result<Vec<String>> {
    let rows = conn.query(
        "SELECT reference FROM fact_citations WHERE fact_id = $1 ORDER BY position",
        &[&fact_id],
    )?;
    Ok(rows.iter().map(|row| row.get(0)).collect())
}

fn attach_citations(conn: &mut Client, facts: &mut [Fact]) -> Result<()> {
    let rows = conn.query(
        "SELECT fact_id, reference FROM fact_citations ORDER BY fact_id, position",
        &[],
    )?;
    let mut by_id: std::collections::BTreeMap<String, Vec<String>> = std::collections::BTreeMap::new();
    for row in rows {
        by_id.entry(row.get(0)).or_default().push(row.get(1));
    }
    for fact in facts.iter_mut() {
        if let Some(citations) = by_id.remove(&fact.id) {
            fact.citations = citations;
        }
    }
    Ok(())
}

fn mint_token(user_id: &str) -> String {
    use std::io::Read;
    let mut bytes = [0u8; 16];
    let _ = std::fs::File::open("/dev/urandom").and_then(|mut file| file.read_exact(&mut bytes));
    let hex: String = bytes.iter().map(|byte| format!("{byte:02x}")).collect();
    format!("dg1.{user_id}.{hex}")
}

fn label_in_use(conn: &mut Client, id: &str) -> Result<bool> {
    let facts = conn.query("SELECT formula FROM facts WHERE formula IS NOT NULL", &[])?;
    for row in facts {
        let formula: String = row.get(0);
        if mentions_atom(&formula, id) {
            return Ok(true);
        }
    }
    let asserts = conn.query("SELECT formula FROM asserts", &[])?;
    for row in asserts {
        let formula: String = row.get(0);
        if mentions_atom(&formula, id) {
            return Ok(true);
        }
    }
    let rules = conn.query("SELECT conclusion FROM rules", &[])?;
    for row in rules {
        let conclusion: String = row.get(0);
        if mentions_atom(&conclusion, id) {
            return Ok(true);
        }
    }
    let premises = conn.query("SELECT premise_id FROM rule_premises WHERE premise_id = $1", &[&id])?;
    Ok(!premises.is_empty())
}

fn mentions_atom(formula: &str, id: &str) -> bool {
    let needle = format!("fact:{id}");
    let mut rest = formula;
    while let Some(at) = rest.find(&needle) {
        let after = at + needle.len();
        let boundary = rest[after..].chars().next();
        if boundary
            .map(|ch| !ch.is_ascii_alphanumeric() && ch != '_')
            .unwrap_or(true)
        {
            return true;
        }
        rest = &rest[after..];
    }
    false
}

fn fresh_id(tx: &mut Transaction<'_>, base: &str) -> Result<String> {
    let mut id = base.to_string();
    let mut n = 2;
    loop {
        let taken = tx
            .query_opt("SELECT id FROM facts WHERE id = $1", &[&id])?
            .is_some()
            || tx
                .query_opt("SELECT id FROM labels WHERE id = $1", &[&id])?
                .is_some();
        if !taken {
            return Ok(id);
        }
        id = format!("{base}_{n}");
        n += 1;
    }
}

fn write_citations(tx: &mut Transaction<'_>, fact_id: &str, citations: &[String]) -> Result<()> {
    for (position, reference) in citations.iter().enumerate() {
        let position = position as i32;
        tx.execute(
            "INSERT INTO fact_citations (fact_id, position, reference) VALUES ($1, $2, $3)",
            &[&fact_id, &position, &reference],
        )?;
    }
    Ok(())
}

fn premises_for(conn: &mut Client, rule_id: &str) -> Result<Vec<String>> {
    let rows = conn.query(
        "SELECT premise_id FROM rule_premises WHERE rule_id = $1 ORDER BY position",
        &[&rule_id],
    )?;
    Ok(rows.iter().map(|row| row.get(0)).collect())
}

fn attach_premises(conn: &mut Client, rules: &mut [Rule]) -> Result<()> {
    for rule in rules {
        rule.premises = premises_for(conn, &rule.id)?;
    }
    Ok(())
}

fn write_premises(tx: &mut Transaction<'_>, rule_id: &str, premises: &[String]) -> Result<()> {
    for (position, premise) in premises.iter().enumerate() {
        let position = position as i32;
        tx.execute(
            "INSERT INTO rule_premises (rule_id, position, premise_id) VALUES ($1, $2, $3)",
            &[&rule_id, &position, &premise],
        )?;
    }
    Ok(())
}

