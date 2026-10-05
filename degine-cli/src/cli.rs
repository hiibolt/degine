use std::io::{self, IsTerminal, Read};

use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(
    name = "degine",
    version,
    about = "Call a degine library",
    subcommand_required = true,
    arg_required_else_help = true
)]
pub struct Cli {
    /// API base. Defaults to https://degine.hiibolt.com.
    #[arg(
        long,
        env = "DEGINE_URL",
        default_value = "https://degine.hiibolt.com",
        global = true
    )]
    pub url: String,

    /// Bearer token, `dg1.<user id>.<secret>`. Prefer DEGINE_TOKEN.
    #[arg(long, env = "DEGINE_TOKEN", hide_env_values = true, global = true)]
    pub token: Option<String>,

    /// Workspace id for library paths. Prefer DEGINE_WORKSPACE.
    #[arg(long, env = "DEGINE_WORKSPACE", global = true)]
    pub workspace: Option<String>,

    #[command(subcommand)]
    pub command: Command,
}

#[derive(Subcommand)]
pub enum Command {
    /// GET a path.
    Get { path: String },
    /// POST a path. The body is an argument, `-` for stdin, or a pipe.
    Post { path: String, body: Option<String> },
    /// PUT a path. The body is an argument, `-` for stdin, or a pipe.
    Put { path: String, body: Option<String> },
    /// DELETE a path. An optional body is an argument, or `-` for stdin.
    Delete { path: String, body: Option<String> },
}

impl Command {
    pub fn request(self) -> (&'static str, String, Option<String>) {
        match self {
            Command::Get { path } => ("GET", path, None),
            Command::Post { path, body } => ("POST", path, take_body(body, true)),
            Command::Put { path, body } => ("PUT", path, take_body(body, true)),
            Command::Delete { path, body } => ("DELETE", path, take_body(body, false)),
        }
    }
}

fn take_body(body: Option<String>, pipe: bool) -> Option<String> {
    match body {
        Some(body) if body == "-" => read_stdin(),
        Some(body) => Some(body),
        None if pipe && !io::stdin().is_terminal() => read_stdin(),
        None => None,
    }
}

fn read_stdin() -> Option<String> {
    let mut body = String::new();
    io::stdin().read_to_string(&mut body).ok()?;
    if body.is_empty() {
        None
    } else {
        Some(body)
    }
}
