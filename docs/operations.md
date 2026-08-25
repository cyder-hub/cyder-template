# Operations

## Release Build

```bash
just build
```

This builds the Rust release binary and the frontend production bundle. The frontend build also produces compressed sidecars used by the Rust static file service.

## Build the Image

```bash
just docker-build
```

The equivalent direct command is:

```bash
docker build -t cyder-template:local -f Dockerfile .
```

The image stores immutable application artifacts under `/app` and runs as non-root UID and GID `10001`.

## Run with SQLite

```bash
docker run --rm -p 8000:8000 -v "$PWD/.app/docker:/data" cyder-template:local
```

At service startup, the entrypoint creates missing `config`, `db`, `storage`, and `logs` directories under `/data` without recursively changing ownership of an existing volume. Configuration and help commands skip this preparation, so validation works with an empty read-only `/data` mount.

Mounting `/data` preserves every application-owned resource. Advanced deployments can mount individual subdirectories, but every writable path must be accessible to UID and GID `10001`.

The image healthcheck resolves configuration through the application binary before requesting `/readyz`, so YAML or `APP_PORT` changes remain aligned with the running service.

## Run with PostgreSQL

```bash
docker compose up --build
```

Compose builds the local image, starts PostgreSQL with a healthcheck, points `APP_DATABASE_URL` at it, and mounts the application data root. Its fixed credentials, exposed PostgreSQL port, and named volumes are development conveniences rather than production defaults.

Validate deployment configuration before starting the service:

```bash
just check-config
```

See [Configuration](configuration.md) for the complete setting surface and strict validation behavior.

## Graceful Shutdown

The backend handles SIGTERM on Unix and Ctrl-C on every supported platform. The first signal immediately makes `/readyz` return `503` while `/healthz` remains available. Normal requests continue through the readiness propagation delay; the listener then stops accepting connections and drains in-flight requests.

`shutdown_timeout_ms` is the total budget from signal receipt and includes `shutdown_readiness_delay_ms`. The defaults are 8000 ms and 1000 ms. The readiness delay may be zero but must remain shorter than the positive total timeout.

A second signal or expired deadline forces shutdown and returns a non-zero process status. Keep the container runtime or orchestrator termination deadline longer than `shutdown_timeout_ms`. Docker's default 10-second stop timeout accommodates the built-in values.

Shutdown logs expose stable event fields:

- `shutdown_signal_received`
- `shutdown_readiness_disabled`
- `shutdown_drain_started`
- `shutdown_completed`
- `shutdown_forced`

Verify an already-built image with a real SIGTERM:

```bash
just test-container-shutdown
```

## Deployment Boundary

The service assumes TLS termination at the deployment edge and does not emit HSTS. It serves the API and frontend on one origin and does not enable CORS. Persistent application state belongs under the configured data root; logs are currently written to stdout and stderr for collection by the runtime platform.
