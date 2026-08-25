# Architecture

## Application Shape

The project is a backend-first web application. A Rust service owns configuration, persistence, HTTP behavior, and production static asset serving. A Vue single-page application provides the browser interface and talks to the API on the same origin in production.

```text
Browser
  ├─ Vue application from front/dist
  └─ /api, /healthz, /readyz
             │
          Axum service
             │
       Diesel async pool
        ├─ SQLite
        └─ PostgreSQL
```

## Backend

The backend crate is in `server/`:

- `main.rs` starts the process and coordinates shutdown.
- `app.rs` assembles shared state, routes, middleware, and static serving.
- `config.rs` owns configuration loading, validation, and reporting.
- `controller/` owns HTTP request parsing and response DTOs.
- `service/` owns business operations.
- `database/` owns persistence and embedded migrations.
- `schema/` contains backend-specific Diesel schema modules.
- `error.rs` defines the public error boundary.
- `http_middleware.rs` implements request IDs, limits, logging, and security headers.

Keep request parsing out of persistence code and database details out of controllers. Add business rules at the service boundary so they remain testable without depending on HTTP representation.

## Frontend

The Vue 3 application in `front/` uses Vite, TypeScript, Pinia, and Vue Router:

- `src/pages/` owns route-level user experiences.
- `src/services/` owns API calls and shared HTTP behavior.
- `src/store/` owns application state.
- `src/router/` owns browser routes.
- `tests/` contains unit and component tests.
- `e2e/` contains Playwright browser contracts.

Development uses Vite with an explicitly injected backend origin. Production uses the prebuilt `front/dist` directory served by Rust; the frontend location is not runtime-configurable.

## Health and Readiness

- `GET /healthz` reports that the process and HTTP router are alive without querying the database.
- `GET /readyz` reports whether the service accepts traffic and can access its database.

Readiness becomes unavailable immediately after the first shutdown signal, while health remains available through the drain period.

## HTTP Boundary

Every response carries `X-Request-ID`. A caller-provided ID is retained only when it contains 1–128 ASCII letters, digits, dots, underscores, or hyphens; otherwise the service generates a UUID v4.

API and readiness failures use one JSON shape:

```json
{"error":"internal_error","message":"internal server error","request_id":"..."}
```

Client-actionable errors retain stable codes such as `invalid_request`, `unsupported_media_type`, `payload_too_large`, `method_not_allowed`, `request_timeout`, `service_overloaded`, and `readiness_failed`. Internal error details remain in server logs.

The request timeout, immediate concurrency rejection, and request-body limit apply to API and readiness routes. Health and static assets do not consume the API concurrency budget. Request panics become logged internal errors without terminating the process; panics outside the HTTP boundary still terminate it.

One structured access event records the request ID, method, matched route, status, latency, and streamed response size. It excludes query strings, bodies, cookies, authorization values, user agents, and forwarded client addresses.

Responses include a same-origin Content Security Policy and defensive content, framing, referrer, permissions, opener, and resource policies. TLS termination and HSTS belong to the deployment edge. CORS is disabled because the production frontend and API share one origin.

## Static Assets

The frontend build creates Brotli and gzip sidecars when compression reduces eligible assets of at least 1 KiB. Static serving negotiates those representations, emits representation-aware strong ETags, and honors `If-None-Match`.

Hashed assets use long-lived immutable caching. The SPA index and history fallback use `no-cache`. API, health, readiness, and error responses use `no-store` unless a successful dynamic handler explicitly provides another policy.
