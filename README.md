# srvcs-outofrange

The out-of-range orchestrator of the srvcs.cloud distributed standard library.

Its single concern: **range: is value outside [lo, hi]?** It owns the *control
flow* — composing two primitives — but does no logic of its own. It asks
[`srvcs-between`](https://github.com/srvcs/between) whether the value lies inside
the closed interval, then [`srvcs-not`](https://github.com/srvcs/not) to negate
that answer.

```
outofrange(value, lo, hi):
    b = between(value, lo, hi)   # is value inside [lo, hi]?
    return not(b)                # ...then it is outside iff not inside
```

So `outofrange(15, 0, 10) == true` and `outofrange(5, 0, 10) == false`.

Validation is not handled here. This service never calls `srvcs-isnumber`
directly; instead its dependencies validate their own operands, and any `422`
they raise is forwarded verbatim.

## API

| Method | Path | Purpose |
| --- | --- | --- |
| `GET` | `/` | Service identity, concern, and dependency list |
| `POST` | `/` | Compute `outofrange(value, lo, hi)` |
| `GET` | `/healthz` `/readyz` `/metrics` `/openapi.json` | srvcs service standard surface |

```sh
curl -s -X POST localhost:8080/ -H 'content-type: application/json' \
  -d '{"value": 15, "lo": 0, "hi": 10}'
# {"value":15.0,"lo":0.0,"hi":10.0,"result":true}
```

Responses:

- `200 {"value": v, "lo": lo, "hi": hi, "result": b}` — evaluated; `result` is a
  boolean.
- `422` — a dependency rejected the input, forwarded verbatim.
- `500` — a reachable dependency returned a `200` without a boolean `result`
  (a contract violation).
- `503` — a dependency is unavailable.

## Dependencies

- [`srvcs-between`](https://github.com/srvcs/between)
- [`srvcs-not`](https://github.com/srvcs/not)

## Configuration

| Variable | Default | Purpose |
| --- | --- | --- |
| `SRVCS_BIND_ADDR` | `0.0.0.0:8080` | Bind address |
| `SRVCS_BETWEEN_URL` | `http://127.0.0.1:8090` | Base URL of `srvcs-between` |
| `SRVCS_NOT_URL` | `http://127.0.0.1:8091` | Base URL of `srvcs-not` |
| `SRVCS_ENV` | `development` | Environment label for logs |
| `RUST_LOG` | `info,tower_http=info` | Tracing filter |

## Local checks

```sh
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
```

Orchestration tests stand up *computing* mock `srvcs-between` and `srvcs-not`
services in-process — they read the request body and return the real
`lo <= value <= hi` / `!value`, so the composition is genuinely exercised against
the asserted cases. See [`srvcs/platform`](https://github.com/srvcs/platform) for
the shared standard.

> Note: the `cargoHash` in `flake.nix` is inherited from the template and must be
> refreshed with a `nix build` before the Nix gates pass.
