# Project Initialization

This repository is a GitHub project template. Initialization is a one-time conversion from the reusable source into an independent application repository.

## Create the Repository

1. Create a repository with GitHub's **Use this template** action.
2. Clone the new repository.
3. Before making project-specific changes, run:

   ```bash
   just init
   ```

You can prefill the lowercase kebab-case project slug while keeping the remaining prompts interactive:

```bash
just init my-api
```

The wizard asks for the project slug, display name, short project description, GitHub repository, whether to run `just check`, and final confirmation.

## Preconditions

Initialization requires a clean Git worktree, including staged and untracked files. It also refuses to run when any of these local paths exists:

- `.app/`
- `target/`
- `front/dist/`
- `front/node_modules/`

Move or remove those paths before initializing. The command does not delete local development data or build artifacts for you.

The project slug must be lowercase ASCII kebab-case, start with a letter, avoid Windows reserved names, and remain short enough for the derived PostgreSQL test database name. The GitHub repository must use a standard `owner/repository` identity and must not point back to the source repository.

## What Initialization Changes

The initializer derives and updates the Rust package and binary, npm package, database names, Docker resources, display name, project description, GitHub links, and other tracked identity references.

It also removes:

- the included `items/users` example resources and their migrations;
- initialization commands, scripts, tests, and lifecycle metadata;
- initialization and maintenance documentation;
- initialization-only CI jobs and content markers.

It preserves the backend and frontend application shells, both database backends, empty migration directories, health endpoints, configuration, HTTP infrastructure, development commands, CI, and production-facing documentation.

The operation is transactional across tracked project files. Planning, write, or validation failures restore the original files. It does not rename the checkout directory, modify Git remotes, rewrite history, create commits, or push changes.

## Automation

Automation can provide a reviewed JSON answers file:

```bash
node scripts/init-project.mjs --answers-file /path/to/answers.json
```

The object accepts `projectSlug`, `displayName`, optional `projectDescription`, `githubRepository`, optional boolean `runCheck`, and `confirm: true`. Unknown fields are rejected.

Example:

```json
{
  "projectSlug": "my-api",
  "displayName": "My API",
  "projectDescription": "API service for internal catalog operations.",
  "githubRepository": "example/my-api",
  "runCheck": true,
  "confirm": true
}
```

## After Initialization

Review the changed files, update the repository description if necessary, and commit the initialization as the first project-specific change. Then start development with:

```bash
just bootstrap
just dev
```

Initialization cannot be run a second time because its command and implementation remove themselves after success.
