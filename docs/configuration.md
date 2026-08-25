# Configuration

The backend starts without a configuration file. Supported values resolve from highest to lowest priority:

1. explicitly supported `APP_*` environment variables;
2. an optional YAML configuration file;
3. built-in defaults.

## Configuration Files

`APP_DATA_DIR` is an environment-only bootstrap setting. Source runs default to `.app/dev`, while the container image sets it to `/data`. The backend automatically reads `<APP_DATA_DIR>/config/config.yaml` when that file exists.

When `APP_CONFIG_PATH` is set, that exact path is required. Missing, unreadable, or non-file paths fail immediately. `config.example.yaml` is a comment-only reference and is never loaded automatically.

The application and `just` do not parse `.env` files. Inject environment values through the shell, container runtime, systemd, Kubernetes, or another deployment tool.

## Environment Variables

| Setting | Default |
| --- | --- |
| `APP_DATA_DIR` | `.app/dev` from source; `/data` in the image |
| `APP_CONFIG_PATH` | `<data-dir>/config/config.yaml`, optional |
| `APP_HOST` | `127.0.0.1` from source; `0.0.0.0` in the image |
| `APP_PORT` | `8000` |
| `APP_DATABASE_URL` | `<data-dir>/db/cyder-template.sqlite` |
| `APP_LOG_LEVEL` | `info` |

`host`, `port`, `database_url`, and `log_level` use the same defaults in YAML and may be overridden by their environment equivalents.

## Operational YAML Settings

| Setting | Default | Rule |
| --- | --- | --- |
| `database_pool_size` | SQLite `1`; PostgreSQL `5` | Greater than `0`; in-memory SQLite requires `1` |
| `database_acquire_timeout_ms` | `30000` | Greater than `0` |
| `sqlite_busy_timeout_ms` | `5000` | SQLite only; `0` disables lock waiting |
| `shutdown_readiness_delay_ms` | `1000` | At least `0` and less than the shutdown timeout |
| `shutdown_timeout_ms` | `8000` | Greater than the readiness delay |
| `http_request_timeout_ms` | `30000` | `1..=300000` |
| `http_max_concurrent_requests` | `64` | `1..=4096` |
| `http_max_request_body_bytes` | `1048576` | `1..=67108864` |

YAML-only settings intentionally have no environment-variable aliases.

## Examples

```bash
APP_PORT=9000 just dev
APP_CONFIG_PATH=/path/to/config.yaml just dev-backend
```

Validate deployment configuration without connecting to a database, running migrations, creating files, or opening a listener:

```bash
just check-config
cargo run -p cyder-template -- config check --format json
```

Normal startup warns about unknown YAML fields and unsupported `APP_*` names without logging their values. This tolerates temporary skew during rolling updates. `config check` is stricter and rejects every ignored setting, making it suitable for deployment and CI gates.

PostgreSQL summaries expose only the backend kind and effective pool settings; database URLs and passwords are not printed.

## Development Endpoint Contract

The binary exposes its non-sensitive resolved listen endpoint for local tooling:

```bash
cargo run -p cyder-template -- config endpoint --format json
# {"host":"127.0.0.1","port":8000}
```

`just dev` consumes this contract. Unspecified bind addresses are converted to usable loopback addresses: `0.0.0.0` becomes `127.0.0.1`, and `::` becomes `::1`. Other hosts and the resolved port are preserved. Port `0` cannot be used for automatic proxy derivation.

`DEV_PROXY_TARGET` belongs to local frontend orchestration and is not an application setting. Production serves the built frontend and API from the same Rust process and origin.

## Data and Asset Paths

Source and release runs serve frontend assets from `front/dist`; the image stores the same immutable artifact at `/app/front/dist`. Persisted state belongs under the data root in `config/`, `db/`, `storage/`, and the reserved `logs/` directory. Temporary files belong under `/tmp/cyder-template`.
