# Wave 3 Integration API Contract (GitLab + Linear)

This document captures the current HTTP contract for the integration routes wired in `at-bridge`.

## Credential resolution

- GitLab token source: `settings.integrations.gitlab_token_env` (resolved from process environment).
- Linear key source: `settings.integrations.linear_api_key_env` (resolved from process environment).
- Missing credentials return `503 Service Unavailable` with:
  - `error` string
  - `env_var` containing the missing environment variable name

## GitLab routes

### `GET /api/gitlab/issues`

Query:
- `project_id` (optional; falls back to `settings.integrations.gitlab_project_id`)
- `state` (optional; defaults to `opened`)
- `page` (optional; defaults to `1`)
- `per_page` (optional; defaults to `20`)

Error behavior:
- `503` when token env var is missing.
- `400` when project ID is missing in both query and settings.

### `GET /api/gitlab/merge-requests`

Query:
- `project_id` (optional; falls back to `settings.integrations.gitlab_project_id`)
- `state` (optional; defaults to `opened`)
- `page` (optional; defaults to `1`)
- `per_page` (optional; defaults to `20`)

Error behavior:
- `503` when token env var is missing.
- `400` when project ID is missing in both query and settings.

### `POST /api/gitlab/merge-requests/{iid}/review`

Body (all optional):
- `project_id` (falls back to `settings.integrations.gitlab_project_id`)
- `strict` (bool)
- `severity_threshold` (`info|low|medium|high|critical`)

Response:
- `MrReviewResult` JSON with `findings[]`, `summary`, `approved`, `reviewed_at`.

Error behavior:
- `503` when token env var is missing.
- `400` when project ID is missing in both body and settings.

## Linear routes

### `GET /api/linear/issues`

Query:
- `team_id` (optional; falls back to `settings.integrations.linear_team_id`)
- `state` (optional)

Error behavior:
- `503` when API key env var is missing.
- `400` when team ID is missing in both query and settings.

### `POST /api/linear/import`

Body:
- `issue_ids`: `string[]` (required)

Response:
- Array of `ImportResult` records with per-issue success/error status.

Error behavior:
- `503` when API key env var is missing.

## Frontend coverage status

- GitHub PRs page now includes a GitLab MRs sub-tab and `Review MR` action.
- API client exposes:
  - `fetch_gitlab_merge_requests(...)`
  - `review_gitlab_merge_request(...)`
