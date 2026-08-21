default: list

# Show available local shortcuts.
list:
	@just --list

# Check the required toolchain, optional container tooling, and local ports.
doctor:
	#!/usr/bin/env bash
	set -euo pipefail
	if ! command -v node >/dev/null 2>&1; then
	  echo "[fail] Node.js is required; install the version in .node-version and ensure node is on PATH." >&2
	  exit 1
	fi
	exec node '{{justfile_directory()}}/scripts/doctor.mjs'

# Prepare a clean checkout for local development.
bootstrap: doctor
	node '{{justfile_directory()}}/scripts/bootstrap.mjs'

# Test toolchain metadata, environment diagnostics, and bootstrap behavior.
test-toolchain:
	node --test "{{justfile_directory()}}/scripts/toolchain.test.mjs"

# Run backend and frontend dev servers together.
dev:
	#!/usr/bin/env bash
	set -euo pipefail
	cd '{{justfile_directory()}}'
	proxy_target="$(node scripts/resolve-dev-proxy.mjs)"

	backend_pid=""
	frontend_pid=""
	started_pid=""

	start_recipe() {
	  local recipe="$1"
	  local recipe_proxy_target="${2:-}"
	  if [[ -n "$recipe_proxy_target" ]] && command -v setsid >/dev/null 2>&1; then
	    DEV_PROXY_TARGET="$recipe_proxy_target" setsid just "$recipe" &
	  elif [[ -n "$recipe_proxy_target" ]]; then
	    DEV_PROXY_TARGET="$recipe_proxy_target" just "$recipe" &
	  elif command -v setsid >/dev/null 2>&1; then
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
	start_recipe dev-front "$proxy_target"
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
	exec cargo run -p cyder-music

# Resolve the backend endpoint, ensure frontend deps, and run the Vite dev server.
dev-front:
	#!/usr/bin/env bash
	set -euo pipefail
	cd '{{justfile_directory()}}'
	proxy_target="$(node scripts/resolve-dev-proxy.mjs)"
	just install-front-deps
	exec env DEV_PROXY_TARGET="$proxy_target" npm --prefix front run dev

# Synchronize locked frontend dependencies for iterative development.
install-front-deps:
	#!/usr/bin/env bash
	set -euo pipefail
	cd '{{justfile_directory()}}'
	marker="front/node_modules/.package-lock.json"
	if [[ ! -f "$marker" || front/package.json -nt "$marker" || front/package-lock.json -nt "$marker" ]]; then
	  npm --prefix front ci
	fi

# Install locked frontend dependencies for verification/builds.
front-ci-deps:
	npm --prefix '{{justfile_directory()}}/front' ci

# Build backend and frontend release artifacts.
build: build-backend build-front

# Build the backend release binary.
build-backend:
	cd '{{justfile_directory()}}' && cargo build -p cyder-music --release

# Build frontend assets from locked dependencies.
build-front: front-ci-deps
	npm --prefix '{{justfile_directory()}}/front' run build

# Run backend and frontend tests.
test: test-backend test-front

# Run backend tests.
test-backend:
	cd '{{justfile_directory()}}' && cargo test -p cyder-music

# Run PostgreSQL integration tests against an isolated test database.
test-postgres:
	#!/usr/bin/env bash
	set -euo pipefail
	cd '{{justfile_directory()}}'
	if [[ -z "${DEV_POSTGRES_TEST_URL:-}" ]]; then
	  echo "DEV_POSTGRES_TEST_URL must point to an isolated PostgreSQL test database." >&2
	  exit 2
	fi
	DEV_POSTGRES_TEST_URL="$DEV_POSTGRES_TEST_URL" cargo test -p cyder-music postgres -- --ignored --test-threads=1

# Validate the resolved deployment configuration without side effects.
check-config:
	cd '{{justfile_directory()}}' && cargo run --quiet --locked -p cyder-music -- config check

# Run frontend checks.
test-front: front-ci-deps
	npm --prefix '{{justfile_directory()}}/front' test

# Run the local aggregate verification suite.
check: test-toolchain fmt-check lint-backend test-backend test-front build-front

# Audit locked dependencies and dependency policy.
audit:
	#!/usr/bin/env bash
	set -euo pipefail
	required_version="0.20.2"
	if ! installed_version="$(cargo deny --version 2>/dev/null)"; then
	  echo "cargo-deny ${required_version} is required." >&2
	  echo "Install it with: cargo install --locked cargo-deny --version ${required_version}" >&2
	  exit 2
	fi
	if [[ "$installed_version" != "cargo-deny ${required_version}" ]]; then
	  echo "cargo-deny ${required_version} is required; found: ${installed_version}" >&2
	  echo "Install it with: cargo install --locked cargo-deny --version ${required_version}" >&2
	  exit 2
	fi
	node '{{justfile_directory()}}/scripts/validate-security-exceptions.mjs'
	cd '{{justfile_directory()}}'
	cargo deny --locked check
	npm --prefix front audit --package-lock-only --audit-level=high

# Check backend compilation without producing release artifacts.
check-backend:
	cd '{{justfile_directory()}}' && cargo check -p cyder-music

# Run strict backend lints across every workspace target and feature.
lint-backend:
	cd '{{justfile_directory()}}' && cargo clippy --workspace --all-targets --all-features --locked -- -D warnings

# Format Rust sources.
fmt:
	cd '{{justfile_directory()}}' && cargo fmt

# Check Rust formatting without writing changes.
fmt-check:
	cd '{{justfile_directory()}}' && cargo fmt --check

# Build the local Docker image.
docker-build image="cyder-music:local":
	cd '{{justfile_directory()}}' && docker build -t "{{image}}" -f Dockerfile .

# Verify graceful SIGTERM handling in an already-built container image.
test-container-shutdown image="cyder-music:local":
	bash '{{justfile_directory()}}/scripts/test-container-shutdown.sh' "{{image}}"

# Verify the safe configuration and directory contract in an already-built image.
test-container-config image="cyder-music:local":
	bash '{{justfile_directory()}}/scripts/test-container-config.sh' "{{image}}"

# Verify HTTP boundaries and static assets in an already-built image.
test-container-http image="cyder-music:local":
	bash '{{justfile_directory()}}/scripts/test-container-http.sh' "{{image}}"
