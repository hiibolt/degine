---
name: degine
description: Read and edit a degine citation-backed argument library (facts, criteria, theorems, labels, and asserts checked by Lean). Use when the user mentions degine, a claim chain, a proof, or /degine.
license: MIT
compatibility: Requires the degine CLI on PATH (cargo install degine), DEGINE_TOKEN, and network access to the degine API.
metadata:
  author: hiibolt
  version: "0.1.0"
---

# degine

Arguments are citation-backed claims checked by Lean. Use the `degine` binary on PATH. If it is missing, install it with `cargo install degine`. Inside the degine repo, `cargo run --manifest-path degine-cli/Cargo.toml --` runs this checkout. `DEGINE_TOKEN` is `dg1.<user id>.<secret>` from the account dialog. The user id is the middle segment. Do not print the token. `DEGINE_URL` defaults to `https://degine.hiibolt.com`. `DEGINE_WORKSPACE` is the workspace id. Library paths (`/facts`, `/rules`, `/labels`, `/asserts`, `/people`, `/personal-facts`, `/comments`, `/inbox`) are prefixed with `/workspaces/$DEGINE_WORKSPACE`. `/me`, `/account/token`, and `/workspaces` are not.

```sh
degine get /workspaces
degine get /facts
degine post /facts '{"id":"signed","claim":"the contract was signed","citations":["https://example.com"],"role":"fact"}'
```

A JSON body can be an argument, `-`, or stdin. Non-2xx exits 1.

## Routes

- `get /me`
- `get /account/token`, `post /account/token`
- `get|post /workspaces`, `put|delete /workspaces/{id}`, `post /workspaces/{id}/leave`
- `get|post /workspaces/{id}/members`, `put|delete /workspaces/{id}/members/{user_id}`
- `get|post /facts`, `put|delete /facts/{id}`, `post /facts/{id}/derive`
- `get|post /rules`, `put|delete /rules/{id}`, `get /rules/{id}/graph`
- `get|put /labels`, `delete /labels/{id}`
- `get|post /asserts`, `put|delete /asserts/{id}`, `get /asserts/{id}/graph`
- `get /asserts/{id}/for/{person}`
- `get|post /people`, `delete /people/{id}`
- `get|post /personal-facts`, `delete /personal-facts/{id}`
- `put /people/{person}/toggles/{fact}`
- `get /comments?target_type=fact&target_id={id}`, `post /comments`
- `put|delete /comments/{id}`, `post /comments/{id}/resolved`
- `get /inbox`

A reader can look and comment. An editor can change the library. Only the creator can rename, delete, invite, or change roles. Invites are by username and start as reader.

## Library

Read `get /facts`, `get /labels`, and `get /asserts` before writing. Ids are slugs: lowercase, `[a-z0-9_]`, stable. Renaming a title must not change an id.

- **fact**: a cited atom. `role` is `fact`, no formula. Citations are a list of urls or notes. Empty citations are allowed only if the user accepts that.
- **criterion**: title only, `role` `criterion`, no formula, no citations. It is assumed only while an assert puts it on the if-side. It is not assumed false.
- **theorem**: an if-then warrant. `formula` is required, `role` `theorem`. The claim is the prose. Citations from a fact that was converted live here.
- **label**: an outcome name, `put /labels` with a JSON object of id to title. The then-side of a theorem is a label or some other atom, never a fresh rename of a fact.
- **assert**: the claim to check. `title`, `description`, `formula`. `get /asserts/{id}/graph` is `pending`, `proved`, or `invalid`.

Formula atoms are `fact:<id>` for facts, criteria, and labels. `imp(fact:a, fact:b)` is if a then b. `and(...)` and `or(...)` take two or more atoms. `or` of more than two is still one `or(...)` list.

Premises of a theorem must already exist. Creating a theorem whose then-atom does not exist is done by putting that label first. Asserts do not mint atoms.

`post /facts/{id}/derive` with `{"id":"<new fact id>","claim":"<lower fact>"}` turns that fact into a theorem. The old id stays the conclusion label. The old wording and citations move onto the theorem, which depends on the new fact.

## How to help

1. Read the library. Say which outcome the user cares about and which facts already reach it.
2. Brainstorm in prose. Name the missing lower facts and the if-then steps. Ask before inventing a personal claim or a citation.
3. Write only what the user agreed to. Create the new facts, then the labels, then the theorems, then the assert.
4. `get /asserts/{id}/graph`. If it is `invalid`, read `diagnostics` and fix the formula. Do not keep adding theorems to paper over a wrong if-then.
5. Comments are `post /comments` with `target_type` `fact`, `assert`, `rule`, or `conclusion`, plus `target_id` and `body`.
