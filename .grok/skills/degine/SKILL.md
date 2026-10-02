---
name: degine
description: Use the degine CLI to read and edit a citation-backed argument library. Brainstorm a chain, then write facts, criteria, theorems, and asserts.
---

# degine

Arguments are citation-backed claims checked by Lean. Use the `degine` binary in `degine-cli` (`cargo run -p` does not apply; run `cargo run --manifest-path degine-cli/Cargo.toml --`). `DEGINE_TOKEN` is `dg1.<user id>.<secret>` from the account dialog. The user id is the middle segment. `DEGINE_URL` defaults to `https://degine.hiibolt.com`.

```sh
degine get /facts
degine post /facts '{"id":"signed","claim":"the contract was signed","citations":["https://example.com"],"role":"fact"}'
```

A JSON body can be an argument or stdin. Non-2xx exits 1. Do not print the token.

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
