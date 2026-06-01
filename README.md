# srvcs-outofrange

## Name

| Field | Value |
| --- | --- |
| Service | `srvcs-outofrange` |
| Slug | `outofrange` |
| Repository | `srvcs/outofrange` |
| Package | `srvcs-outofrange` |
| Kind | `orchestrator` |

## Function

range: is value outside [lo, hi]

## Dependencies

| Dependency | Repository |
| --- | --- |
| `srvcs-between` | [srvcs/between](https://github.com/srvcs/between) |
| `srvcs-not` | [srvcs/not](https://github.com/srvcs/not) |

## API

| Method | Path | Purpose |
| --- | --- | --- |
| `GET` | `/` | Service identity |
| `POST` | `/` | Evaluate the service function |
| `GET` | `/healthz` | Liveness probe |
| `GET` | `/readyz` | Readiness probe |
| `GET` | `/metrics` | Prometheus metrics |
| `GET` | `/openapi.json` | OpenAPI document |

## Inputs

| Name | Type | Required |
| --- | --- | --- |
| `value` | `number` | yes |
| `lo` | `number` | yes |
| `hi` | `number` | yes |

## Outputs

| Name | Type |
| --- | --- |
| `value` | `number` |
| `lo` | `number` |
| `hi` | `number` |
| `result` | `boolean` |

## Configuration

| Variable | Default | Purpose |
| --- | --- | --- |
| `SRVCS_BIND_ADDR` | `0.0.0.0:8080` | Bind address |
| `SRVCS_ENV` | `development` | Environment label for logs |
| `RUST_LOG` | `info,tower_http=info` | Tracing filter |
| `SRVCS_BETWEEN_URL` | `http://127.0.0.1:8090` | Base URL for srvcs-between |
| `SRVCS_NOT_URL` | `http://127.0.0.1:8091` | Base URL for srvcs-not |

## Error Behavior

- `422` means the request could not be evaluated for the documented input shape.
- `503` means a required dependency was unavailable or returned an unexpected response.
- Dependency validation errors are forwarded when this service delegates validation.

## Local Checks

```sh
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
```

See the [srvcs service standard](https://github.com/srvcs/platform/blob/main/STANDARD.md) for the full operational contract.

## Metadata

Machine-readable service metadata lives in `srvcs.yaml`. Keep it aligned with this README when the service contract changes.
