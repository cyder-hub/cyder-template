## Summary

-

## Verification

Commands run:

- [ ] `cargo fmt --check`
- [ ] `cargo check --workspace --locked`
- [ ] `cargo test --workspace --locked`
- [ ] `DEV_POSTGRES_TEST_URL=... just test-postgres` (database changes that affect PostgreSQL)
- [ ] `npm --prefix front ci`
- [ ] `npm --prefix front test`
- [ ] `npm --prefix front run build`
- [ ] `just audit`
- [ ] `docker compose -f docker-compose.yml config`
- [ ] `docker build -t cyder-template:ci -f Dockerfile .`
- [ ] `bash scripts/test-container-config.sh cyder-template:ci` (runtime configuration or packaging changes)
- [ ] Not run; reason:

## Impact

- [ ] Backend Rust code
- [ ] Frontend Vue code
- [ ] Database migrations or schema
- [ ] Docker or compose
- [ ] GitHub Actions or repository metadata
- [ ] Documentation only
- [ ] Project identity or naming
- [ ] Dependencies or lockfiles

## Checklist

- [ ] I did not commit `front/node_modules/`, `front/dist/`, `target/`, `.app/`, `.env`, local databases, logs, or credentials.
- [ ] I updated README or contributing guidance where behavior, commands, naming, or dependencies changed.
- [ ] I listed SQLite and PostgreSQL verification results for database behavior changes, or explained why a backend was not run.
- [ ] I avoided adding claims for unimplemented features.
