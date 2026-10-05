use std::time::{Duration, Instant};

use anyhow::{anyhow, Context, Result};
use jsonwebtoken::jwk::Jwk;
use jsonwebtoken::{decode, decode_header, Algorithm, DecodingKey, Validation};
use serde::Deserialize;

#[derive(Clone)]
pub struct Auth {
    issuer: String,
    keys: std::sync::Arc<tokio::sync::Mutex<Option<(Instant, Vec<Jwk>)>>>,
    client: reqwest::Client,
}

#[derive(Debug, Clone)]
pub struct Person {
    pub id: String,
    pub email: String,
    pub username: Option<String>,
}

#[derive(Deserialize)]
struct Jwks {
    keys: Vec<Jwk>,
}

#[derive(Deserialize)]
struct Meta {
    username: Option<String>,
}

#[derive(Deserialize)]
struct Claims {
    sub: String,
    email: Option<String>,
    user_metadata: Option<Meta>,
}

impl Auth {
    pub fn new(supabase_url: &str) -> Self {
        let base = supabase_url.trim_end_matches('/');
        Self {
            issuer: format!("{base}/auth/v1"),
            keys: std::sync::Arc::new(tokio::sync::Mutex::new(None)),
            client: reqwest::Client::new(),
        }
    }

    pub async fn verify(&self, token: &str) -> Result<Person> {
        let header = decode_header(token).context("could not read the token header")?;
        if header.alg != Algorithm::ES256 {
            return Err(anyhow!("unexpected token algorithm"));
        }
        let kid = header.kid.ok_or_else(|| anyhow!("token has no key id"))?;
        let jwk = self.key(&kid).await?;
        let key = DecodingKey::from_jwk(&jwk).context("could not use the signing key")?;
        let mut validation = Validation::new(Algorithm::ES256);
        validation.set_issuer(&[&self.issuer]);
        validation.set_audience(&["authenticated"]);
        let data = decode::<Claims>(token, &key, &validation).context("token was rejected")?;
        let email = data
            .claims
            .email
            .unwrap_or_default()
            .trim()
            .to_lowercase();
        let id = data.claims.sub.trim().to_string();
        if id.is_empty() || email.is_empty() || !email.contains('@') {
            return Err(anyhow!("token has no email"));
        }
        let username = data
            .claims
            .user_metadata
            .and_then(|meta| meta.username)
            .map(|name| name.trim().to_lowercase())
            .filter(|name| !name.is_empty());
        Ok(Person { id, email, username })
    }

    async fn key(&self, kid: &str) -> Result<Jwk> {
        let mut cache = self.keys.lock().await;
        let fresh = cache
            .as_ref()
            .is_some_and(|(at, _)| at.elapsed() < Duration::from_secs(3600));
        if !fresh {
            let keys = self
                .client
                .get(format!("{}/.well-known/jwks.json", self.issuer))
                .send()
                .await
                .context("could not fetch signing keys")?
                .error_for_status()
                .context("signing keys were refused")?
                .json::<Jwks>()
                .await
                .context("signing keys were unreadable")?;
            *cache = Some((Instant::now(), keys.keys));
        }
        cache
            .as_ref()
            .and_then(|(_, keys)| keys.iter().find(|key| key.common.key_id.as_deref() == Some(kid)))
            .cloned()
            .ok_or_else(|| anyhow!("signing key was not found"))
    }
}
