use std::time::Duration;

use ureq::http;
use ureq::Agent;

pub struct Client {
    base: String,
    token: String,
    agent: Agent,
}

pub enum CallError {
    Status { code: u16, body: String },
    Transport(String),
}

impl Client {
    pub fn new(base: String, token: String) -> Self {
        let config = Agent::config_builder()
            .timeout_global(Some(Duration::from_secs(30)))
            .http_status_as_error(false)
            .build();
        Self {
            base,
            token,
            agent: config.into(),
        }
    }

    pub fn call(&self, method: &str, path: &str, body: Option<&str>) -> Result<String, CallError> {
        let url = join(&self.base, path);
        let auth = format!("Bearer {}", self.token);
        let ua = format!("degine/{}", env!("CARGO_PKG_VERSION"));
        let result = if let Some(body) = body {
            let request = http::Request::builder()
                .method(method)
                .uri(&url)
                .header("authorization", &auth)
                .header("user-agent", &ua)
                .header("content-type", "application/json")
                .body(body)
                .map_err(|err| CallError::Transport(err.to_string()))?;
            self.agent.run(request)
        } else {
            let request = http::Request::builder()
                .method(method)
                .uri(&url)
                .header("authorization", &auth)
                .header("user-agent", &ua)
                .body(())
                .map_err(|err| CallError::Transport(err.to_string()))?;
            self.agent.run(request)
        };
        let mut response = result.map_err(|err| CallError::Transport(err.to_string()))?;
        let code = response.status().as_u16();
        let text = response
            .body_mut()
            .read_to_string()
            .map_err(|err| CallError::Transport(err.to_string()))?;
        if (200..300).contains(&code) {
            let sniff = text.trim_start();
            if sniff.starts_with('<') {
                return Err(CallError::Transport(
                    "api returned html. the server is the site, not the library".into(),
                ));
            }
            Ok(text)
        } else {
            Err(CallError::Status { code, body: text })
        }
    }
}

fn join(base: &str, path: &str) -> String {
    let base = base.trim_end_matches('/');
    if path.starts_with('/') {
        format!("{base}{path}")
    } else {
        format!("{base}/{path}")
    }
}
