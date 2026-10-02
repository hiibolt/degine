# Debate Logic Tool — Build Plan

## What This Is

A web application for resolving factual disagreements by building a formally
verified, citation-backed chain of reasoning. Rather than arguing back and forth
about whether a claim is true, you record the underlying facts (each tied to a
source such as a message, document, or timestamp), define the logical rules that
connect them, and let a trusted proof-checking engine confirm whether a given
conclusion actually follows from those facts. The output is a chain that can be
visually inspected, conclusion back down to its cited sources, node by node.

The app is shared and collaborative: other people can log in, inspect the proof
chain, and leave comments on any fact, rule, or conclusion. The owner can edit
facts and rules in response, and everyone sees updates in real time.

## Why It Exists (Background)

The motivating problem: disagreements with someone skilled in debate and rhetoric
tend to stall not because the facts are unclear, but because logical fallacies get
mixed in with real argumentation, and it's hard in the moment to cleanly separate
"this is wrong" from "I can't articulate why fast enough." The goal is a tool that
converts an argument into something checkable rather than rhetorical.

Reasoning disputes generally break down into two separate problems: whether the
underlying facts are true, and whether the logic connecting them to a conclusion
is valid. This tool does not solve the first problem. Someone still has to judge
whether a cited message or document actually supports a claim, and that remains a
human, subjective judgment call. What it does solve is the second problem: once
facts and rules are entered, the validity of the resulting chain is checked
mechanically, not argued over.

The comment system exists because the first problem is still a human one: the
other party needs a place to say "I don't think this citation supports this
fact" directly on the node in question, and the owner needs to be able to respond
by changing the fact set while both watch the effect live.

## Prior Art Considered

- Argument-mapping tools (Kialo, Rationale, MindMup, FigJam templates): these
  visualize claims, premises, objections, and evidence, but have no formal
  verification layer. Validity of the reasoning is still a matter of human
  opinion, which doesn't solve the core problem.
- Carneades: an existing computational argumentation framework built around
  defeasible logic, closest in spirit to what's wanted, but not a fit for a
  from-scratch Rust/Svelte build.
- A hand-rolled logic engine written directly in Rust: functionally sufficient,
  but rejected as the proof-checking engine because custom code is easy to
  dismiss as potentially buggy, undermining the credibility of any conclusion it
  produces.

## Design Decision: Why Lean

Lean was chosen as the proof-checking backend specifically for credibility
reasons. Its trusted kernel (the part that actually checks a proof is valid) is
small and has been independently audited, and Lean is used in serious formal
mathematics (e.g. verifying the four-color theorem). A conclusion that compiles
under Lean can't be casually waved away as "there's probably a bug in your code"
the way a custom Rust implementation could be. The tradeoff: Lean guarantees the
logic is airtight, but cannot verify that a cited source actually says what you
claim it says. That judgment call stays with the humans in the room.

## Goals

- Make the validity of a conclusion mechanically checkable, not a matter of opinion.
- Tie every base fact to a concrete, inspectable source (a link to a message, a
  document, etc.).
- Visualize the full reasoning chain so anyone can click through from a
  conclusion down to the exact facts and rules it depends on.
- Let invited people comment on any node, and let the owner edit in response,
  with every connected client updating in real time.

## Architecture Overview

A standard client/server split. This replaces the earlier desktop (Tauri) design.

- **Backend:** Rust. Owns the database, auth, the Lean pipeline, REST endpoints,
  and a WebSocket endpoint for real-time push.
- **Frontend:** Svelte (via Vite) with Bun as the package manager and script
  runner. A single-page app that talks to the backend over REST and WebSocket.
- **Lean helper:** a standalone Lean executable, built with Lake, invoked by the
  backend as a subprocess.

## Cross-Cutting Conventions

### Error handling: anyhow everywhere, propagated up

- Every fallible function in the backend returns `anyhow::Result<T>`.
- Every fallible call gets context via `.context("...")` or
  `.with_context(|| format!(...))`, so the final error reads as a chain
  ("failed to compile theorem X: failed to spawn lean helper: no such file").
- Propagate with `?`; no `unwrap`/`expect` outside tests. `main` returns
  `anyhow::Result<()>`, and config loading, DB setup, and server startup all
  fail loudly through it.
- At the HTTP boundary, handlers return `Result<T, AppError>`, where `AppError`
  is a newtype around `anyhow::Error` that implements axum's `IntoResponse`
  (logs the full chain with `{:#}`, returns a 500 with a short message) and
  `From<E>` for anything convertible into `anyhow::Error`, so `?` works inside
  handlers.
- Expected client errors (bad login, missing record) return explicit status
  codes rather than going through the generic 500 path.
- A Lean *compile failure* caused by an invalid proof chain is a normal result,
  not an error: it's returned as structured data (diagnostics) to the client.
  Failing to *run* Lean at all (missing binary, crash, unreadable output) is an
  anyhow error.
- WebSocket handlers follow the same rule: errors propagate up, get logged with
  full context, and close the offending connection rather than the server.
- The Lean helper itself is Lean code, so anyhow doesn't apply to it. It writes
  errors to stderr and exits non-zero, and the Rust side captures stderr and
  attaches it as context.

### Configuration

Backend config lives in a TOML file loaded at startup (path via CLI arg or a
default like `./config.toml`), parsed with `serde` + `toml`, and every step wrapped
in anyhow context. Example shape:

    [server]
    bind = "127.0.0.1:3000"

    [lean]
    project_dir = "./lean-project"
    helper_path = "./lean-project/.lake/build/bin/extractor"

    [[users]]
    username = "owner"
    password = "change-me"

    [[users]]
    username = "guest"
    password = "change-me-too"

Credentials are plaintext for now (see Deferred). The `[[users]]` list is there so
comments can be attributed to a person; with a single shared credential there'd
be no way to tell who wrote what.

### Frontend environment variables

Endpoints are configured via Vite env vars in a `.env` file (localhost for now):

    VITE_API_URL=http://localhost:3000
    VITE_WS_URL=ws://localhost:3000/ws

Read in code through `import.meta.env.VITE_API_URL` and
`import.meta.env.VITE_WS_URL`. No hardcoded URLs anywhere in the frontend.

## Backend

### Suggested crates

axum (with the `ws` feature), tokio, serde + serde_json, anyhow, toml, tracing,
tower-http (CORS is required, since the Vite dev server and the backend run on
different ports), and either rusqlite or sqlx for SQLite.

### Data layer (SQLite)

- `facts`: id, claim text, citations (one or more URLs or message references), timestamp.
- `rules`: id, list of premise fact/rule IDs, conclusion.
- `comments`: id, target type (fact / rule / conclusion), target id, author
  (username), body, created_at.

### Auth

- `POST /login` takes username and password, checks them against the TOML user
  list, and returns a random session token held in server memory.
- REST requests send the token as `Authorization: Bearer <token>`.
- Browsers can't set custom headers on a WebSocket handshake, so the WS
  connection passes the token as a query parameter (`/ws?token=...`), validated
  on upgrade.

### REST endpoints

- Auth: `POST /login`.
- Facts and rules: create, edit, delete, list.
- Comments: create, list for a given target.
- Proof graph: fetch the current dependency graph for a conclusion.

### WebSocket (real-time)

- A single `/ws` endpoint. Each connected client subscribes to a
  `tokio::sync::broadcast` channel.
- Mutations go through REST; the WebSocket is server-to-client push. After any
  successful mutation the server broadcasts a JSON event (a tagged enum, e.g.
  `fact_changed`, `rule_changed`, `comment_added`, `graph_updated`,
  `compile_started`).
- Clients apply the event to local state or refetch the affected resource.
- Client-side: auto-reconnect with backoff, and a full refetch on reconnect so
  nothing missed during a disconnect is lost.

### Lean pipeline

1. **Code generation.** Rust translates each stored fact into a Lean `axiom` and
   each rule into a theorem/lemma connecting premises to a conclusion, writing
   `.lean` files into the Lean project directory. Citation metadata isn't
   understood by Lean's logic, so it stays in SQLite (and/or structured
   comments) and is re-associated with each node when the graph is served.
2. **Dependency extractor (Lean).** `#print axioms` reports root axioms only, so
   it can't give the full chain. Instead, a small standalone Lean program,
   built with Lake:
   - uses `Environment.find?` to fetch a declaration's `ConstantInfo` (type and,
     for theorems/defs, the proof term as an `Expr` tree);
   - recursively walks the `Expr`, and on every `const` node records an edge and
     recurses into that declaration;
   - keeps a visited `HashSet` of `Name` to avoid reprocessing and cycles;
   - classifies each dependency (axiom / theorem / def) for node styling;
   - serializes nodes and edges to JSON via `Lean.Json` / `ToJson` on stdout.
3. **Rust-Lean bridge.** The backend spawns the helper with `tokio::process`,
   passing the target theorem name, captures stdout (JSON graph) and stderr, and
   parses the result, with anyhow context at every step.
4. **Serialized job queue.** Because multiple people can edit while compiles are
   slow, Lean work runs through a single worker task fed by an `mpsc` queue.
   That prevents concurrent edits from racing on the `.lean` files. The server
   broadcasts `compile_started` when a job begins and `graph_updated` when it
   finishes.

## Frontend

- Vite + Svelte SPA, with Bun for installs and scripts (`bun install`,
  `bun run dev`, `bun run build`).
- Login screen that stores the session token.
- Fact and rule entry forms (claim text, citation links, optional timestamp).
- Graph visualization (Svelte Flow or Cytoscape.js): nodes for facts and
  conclusions, edges for dependencies, axioms styled differently from derived
  nodes. Clicking a node shows its citations and, for derived nodes, the rule
  that produced it.
- Comment thread panel attached to the selected node, updating live.
- A WebSocket client module that handles connect, reconnect, and dispatching
  events into Svelte stores.

## End-to-End Data Flow

Owner edits a fact in the UI → REST call to the backend → backend writes to
SQLite, regenerates the `.lean` files, and enqueues a Lean job → backend
broadcasts `compile_started` over WebSocket → the Lean worker compiles and
extracts the dependency graph JSON → backend broadcasts `graph_updated` → every
connected client (owner and commenters) re-renders the graph live.

A commenter posts a comment → REST call → stored in SQLite → backend broadcasts
`comment_added` → every connected client sees it appear on the node immediately.

## Key Caveat

Lean guarantees the logical chain is valid. It does not and cannot verify that a
citation actually supports the fact it's attached to, or that a base fact is a
fair characterization of what was actually said. That judgment remains human.
This tool formalizes the logic layer of a disagreement; it does not formalize or
automate the evidentiary layer.

## Deferred (acknowledged, intentionally not in scope yet)

- Plaintext passwords in the config file and in-memory-only sessions: fine for
  localhost development, to be replaced with hashed passwords and persistent
  sessions before real use.
- TLS: everything runs over plain `http://` and `ws://` on localhost. Before
  exposing this beyond localhost, it needs HTTPS/WSS, since credentials and the
  WS token otherwise travel in the clear.
- Concurrent editing conflicts: last write wins for now.
- Per-user permissions (e.g. read-only vs. editor): everyone with a login can
  currently do everything.
