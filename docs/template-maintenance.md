# Template Maintenance

This document applies only while maintaining the reusable source repository. It is deleted from initialized projects.

## Principles

- Production-facing files should read like a normal application repository.
- Keep initialization behavior in `.template-state.json` and `scripts/template-project.mjs`.
- Keep example resources isolated so initialization can remove complete files whenever possible.
- Use lifecycle markers only at unavoidable composition boundaries such as module registration, route assembly, top-level documentation links, and initialization-only CI steps.
- Do not add runtime checks for whether the repository has been initialized. Initialization is a one-time tracked-file transformation, not an application mode.

## Identity Changes

The source identity in `.template-state.json` must match every tracked source identifier. When adding an identity-bearing file or changing the initializer's input model, update the existing initialization tests and every affected lockfile or manifest.

The initializer must continue to reject dirty worktrees and local artifact directories. It must not change the checkout directory, Git remote, commit history, or untracked application data.

## Example Boundaries

The `items/users` resources exist to exercise backend routing, both schema backends, frontend API calls, pages, and the browser path. Initialized projects do not retain those resources.

Prefer dedicated example modules, tests, styles, and migrations that can be listed as whole-file removals. When a shared file must contain example wiring, keep the marked block small and avoid mixing unrelated production behavior into it.

## Verification

Run the existing initialization contract after changing identities, lifecycle files, markers, example resources, or CI initialization steps:

```bash
just test-template-init
```

Run the complete initialized-product path when changing generated output:

```bash
DEV_TEMPLATE_PRODUCT_CHECK=1 node --test scripts/template-project.test.mjs
```

Use `just check` to validate the reusable source itself. See [Verification](verification.md) for the remaining project checks.
