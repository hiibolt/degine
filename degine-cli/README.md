# degine

Command line client for a [degine](https://degine.hiibolt.com) argument library.

```sh
cargo install degine
```

A push to `main` that changes `degine-cli` publishes this crate when the `version` in `Cargo.toml` is not already on crates.io. The backend is not published.

`DEGINE_TOKEN` is the `dg1.<user id>.<secret>` value from the account dialog. `DEGINE_URL` defaults to `https://degine.hiibolt.com`.

```sh
degine get /facts
degine post /facts '{"id":"signed","claim":"the contract was signed","citations":["https://example.com"],"role":"fact"}'
```

A JSON body can be an argument, `-`, or stdin. The process exits 1 when the response is not 2xx.

The agent skill that knows how to edit a library lives in `skills/degine`. Other people install it with:

```sh
npx skills add hiibolt/degine
```
