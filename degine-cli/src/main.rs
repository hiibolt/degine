mod cli;
mod client;

use std::io::{self, Write};
use std::process::exit;

use clap::Parser;

use cli::Cli;
use client::{CallError, Client};

fn main() {
    let cli = Cli::parse();
    let Some(token) = cli.token.filter(|token| !token.is_empty()) else {
        eprintln!("set DEGINE_TOKEN");
        exit(1);
    };
    let (method, path, body) = cli.command.request();
    let path = match scope_path(&path, cli.workspace.as_deref()) {
        Ok(path) => path,
        Err(()) => {
            eprintln!("set DEGINE_WORKSPACE");
            exit(1);
        }
    };
    let client = Client::new(cli.url, token);
    match client.call(&method, &path, body.as_deref()) {
        Ok(body) => write_out(&body),
        Err(CallError::Status { code, body }) => {
            write_out(&body);
            if body.trim().is_empty() {
                eprintln!("degine: {code}");
            }
            exit(1);
        }
        Err(CallError::Transport(err)) => {
            eprintln!("degine: {err}");
            exit(1);
        }
    }
}

fn scope_path(path: &str, workspace: Option<&str>) -> Result<String, ()> {
    let path = if path.starts_with('/') {
        path.to_string()
    } else {
        format!("/{path}")
    };
    if path.starts_with("/workspaces") {
        return Ok(path);
    }
    let library = ["/facts", "/rules", "/labels", "/asserts", "/people", "/personal-facts", "/comments", "/inbox"]
        .iter()
        .any(|prefix| path == *prefix || path.starts_with(&format!("{prefix}/")) || path.starts_with(&format!("{prefix}?")));
    if !library {
        return Ok(path);
    }
    let Some(id) = workspace.filter(|id| !id.is_empty()) else {
        return Err(());
    };
    Ok(format!("/workspaces/{id}{path}"))
}

fn write_out(body: &str) {
    let mut out = io::stdout();
    let _ = out.write_all(body.as_bytes());
    if !body.is_empty() && !body.ends_with('\n') {
        println!();
    }
}
