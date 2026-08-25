# cyder-template

A backend-first Rust and Vue application with SQLite and PostgreSQL support.

<!-- template-init:start -->
> [!IMPORTANT]
> Initialize a new repository before starting product development. See [Project initialization](docs/initialization.md) for the required clean-checkout workflow and its one-time effects.
<!-- template-init:end -->

## Quick Start

Install the pinned tools described in [Development](docs/development.md), then prepare and run the application:

```bash
just bootstrap
just dev
```

Open the Vite URL printed in the terminal. The frontend proxies application and health requests to the Rust backend, which uses SQLite by default.

## Common Commands

```bash
just --list       # show every available command
just bootstrap    # check tools, install frontend dependencies, create local data paths
just dev          # run the backend and frontend development servers
just test         # run backend and frontend tests
just check        # run the standard local quality gates
just build        # build backend and frontend release artifacts
```

See [Development](docs/development.md) and [Verification](docs/verification.md) for specialized commands and change-specific checks.

## Project Layout

```text
server/       Rust application, HTTP API, persistence, and migrations
front/        Vue application, unit tests, and browser tests
scripts/      Development, validation, and container helpers
docker/       Local container support files
docs/         Development and operational documentation
```

Local runtime state is stored under the ignored `.app/` directory. Generated frontend assets, installed dependencies, databases, and logs should not be committed.

## Documentation

Start with the [documentation index](docs/README.md):

- [Development](docs/development.md)
- [Configuration](docs/configuration.md)
- [Database](docs/database.md)
- [Architecture](docs/architecture.md)
- [Verification](docs/verification.md)
- [Operations](docs/operations.md)

## License

Licensed under the MIT License. See [LICENSE](LICENSE).
