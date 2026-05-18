set dotenv-load := true

default: list

# Show available local shortcuts.
list:
	@just --list

# Run backend and frontend dev servers together.
dev:
	#!/usr/bin/env bash
	set -euo pipefail
	cd '{{justfile_directory()}}'

	backend_pid=""
	frontend_pid=""
	started_pid=""

	start_recipe() {
	  local recipe="$1"
	  if command -v setsid >/dev/null 2>&1; then
	    setsid just "$recipe" &
	  else
	    just "$recipe" &
	  fi
	  started_pid="$!"
	}

	stop_process() {
	  local pid="$1"
	  [[ -n "$pid" ]] || return 0

	  if kill -0 "-$pid" 2>/dev/null; then
	    kill -TERM "-$pid" 2>/dev/null || true
	  elif kill -0 "$pid" 2>/dev/null; then
	    kill -TERM "$pid" 2>/dev/null || true
	  fi
	}

	cleanup() {
	  local status="$?"
	  trap - INT TERM EXIT
	  stop_process "$backend_pid"
	  stop_process "$frontend_pid"
	  wait "$backend_pid" 2>/dev/null || true
	  wait "$frontend_pid" 2>/dev/null || true
	  exit "$status"
	}

	trap cleanup INT TERM EXIT

	start_recipe dev-backend
	backend_pid="$started_pid"
	start_recipe dev-front
	frontend_pid="$started_pid"

	while true; do
	  if ! kill -0 "$backend_pid" 2>/dev/null; then
	    set +e
	    wait "$backend_pid"
	    status="$?"
	    set -e
	    exit "$status"
	  fi

	  if ! kill -0 "$frontend_pid" 2>/dev/null; then
	    set +e
	    wait "$frontend_pid"
	    status="$?"
	    set -e
	    exit "$status"
	  fi

	  sleep 1
	done

# Run the backend dev server.
dev-backend:
	#!/usr/bin/env bash
	set -euo pipefail
	cd '{{justfile_directory()}}'
	if [[ -z "${APP_DATA_DIR:-}" ]]; then
	  export APP_DATA_DIR='{{justfile_directory()}}/.app/dev'
	fi
	exec cargo run -p cyder-template

# Ensure frontend deps and run the Vite dev server.
dev-front: install-front-deps
	npm --prefix '{{justfile_directory()}}/front' run dev

# Ensure frontend dependencies for iterative development.
install-front-deps:
	#!/usr/bin/env bash
	set -euo pipefail
	cd '{{justfile_directory()}}'
	marker="front/node_modules/.package-lock.json"
	if [[ ! -f "$marker" || front/package.json -nt "$marker" || front/package-lock.json -nt "$marker" ]]; then
	  npm --prefix front install
	fi

# Install locked frontend dependencies for verification/builds.
front-ci-deps:
	npm --prefix '{{justfile_directory()}}/front' ci

# Build backend and frontend release artifacts.
build: build-backend build-front

# Build the backend release binary.
build-backend:
	cd '{{justfile_directory()}}' && cargo build -p cyder-template --release

# Build frontend assets from locked dependencies.
build-front: front-ci-deps
	npm --prefix '{{justfile_directory()}}/front' run build

# Run backend and frontend tests.
test: test-backend test-front

# Run backend tests.
test-backend:
	cd '{{justfile_directory()}}' && cargo test -p cyder-template

# Run PostgreSQL integration tests against an isolated test database.
test-postgres:
	#!/usr/bin/env bash
	set -euo pipefail
	cd '{{justfile_directory()}}'
	if [[ -z "${APP_TEST_POSTGRES_URL:-}" ]]; then
	  echo "APP_TEST_POSTGRES_URL must point to an isolated PostgreSQL test database." >&2
	  exit 2
	fi
	cargo test -p cyder-template postgres -- --ignored

# Run frontend checks.
test-front: front-ci-deps
	npm --prefix '{{justfile_directory()}}/front' test

# Run the local aggregate verification suite.
check: fmt-check check-backend test-backend test-front build-front

# Check backend compilation without producing release artifacts.
check-backend:
	cd '{{justfile_directory()}}' && cargo check -p cyder-template

# Format Rust sources.
fmt:
	cd '{{justfile_directory()}}' && cargo fmt

# Check Rust formatting without writing changes.
fmt-check:
	cd '{{justfile_directory()}}' && cargo fmt --check

# Build the local Docker image.
docker-build image="cyder-template:local":
	cd '{{justfile_directory()}}' && docker build -t "{{image}}" -f Dockerfile .
