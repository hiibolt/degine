# degine

Citation-backed arguments checked by Lean. Layout and design live in `PLAN.md`.

## Ground rule

Absolutely zero tests.

Do not add unit tests, integration tests, doctests, snapshot tests, `#[cfg(test)]` modules, a `tests/` directory, or test-only dependencies. Do not run `cargo test` or any other test runner. Check work by building the project and running it.
