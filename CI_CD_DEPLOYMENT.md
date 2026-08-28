# CI/CD Deployment Integration

StarForge provides a consistent CI/CD deployment interface for GitHub Actions, GitLab CI, and Jenkins. Each provider runs the same quality gate before it can deploy and delegates infrastructure-specific work to protected CI secrets.

## Quality Gate

The deployment pipelines require:

- `cargo fmt --all --check`
- `cargo build --locked`
- `cargo test --locked`
- `cargo clippy --all-features --locked -- -D warnings`
- `cargo test --test cli_smoke --locked`

The existing deployment verification and rollback harness can be added to project release pipelines when contract artifacts and rollback scenarios are available. See `ROLLBACK_TESTING.md`.

## Resumable and Idempotent Deployments

Deployment operations automatically persist progress checkpoints to disk (`~/.starforge/checkpoints/`). If a CI job is cancelled or interrupted mid-flight, re-running the deployment step automatically resumes from the last succeeded step without re-executing earlier steps. If a deployment has already succeeded, re-running the command is a safe no-op. See [`docs/DEPLOYMENT_CHECKPOINTS.md`](docs/DEPLOYMENT_CHECKPOINTS.md).

## Required CI Secrets

Configure these values as masked/protected secrets (GitHub environment secrets, GitLab protected variables, or Jenkins credentials):

| Name | Purpose |
| --- | --- |
| `STARFORGE_DEPLOY_COMMAND` | Command that deploys the immutable artifact for the selected environment. |
| `STARFORGE_ROLLBACK_COMMAND` | Command that restores the last known-good artifact. |
| `STARFORGE_HEALTHCHECK_URL` | Optional endpoint returning a successful HTTP status once the release is healthy. |

Commands are executed only by manually approved deployment jobs. Do not put tokens directly in the command; use the provider's secret mechanism or workload identity.

## GitHub Actions

Run **Safe Deployment** with `deploy` or `rollback` and choose `staging` or `production`. Create matching GitHub environments and configure required reviewers for `production`; the workflow waits for that approval, serializes work per environment, then uses the environment's secrets.

## GitLab CI

`.gitlab-ci.yml` defines manual staging and default-branch production deployment/rollback jobs. Mark the production environment and variables as protected. `resource_group` prevents overlapping environment changes.

## Jenkins

The `Jenkinsfile` accepts `verify`, `deploy`, or `rollback`. Add Jenkins Secret Text credentials using these IDs:

- `starforge-deploy-command`
- `starforge-rollback-command`
- `starforge-healthcheck-url`

Production runs pause for an explicit Jenkins approval.

## Monitoring and Rollback

`scripts/ci-deploy.sh` polls `STARFORGE_HEALTHCHECK_URL` after deployment. A failed health check fails the job and makes the separately manual rollback action immediately available. The rollback job executes `STARFORGE_ROLLBACK_COMMAND` and checks health once more. Configure an external alert against the same endpoint so operators receive failures beyond CI logs.

## Local Validation

```bash
bash -n scripts/ci-deploy.sh scripts/ci-rollback.sh
```
