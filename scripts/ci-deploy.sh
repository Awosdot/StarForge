#!/usr/bin/env bash
set -euo pipefail

: "${STARFORGE_DEPLOY_ENVIRONMENT:?Set STARFORGE_DEPLOY_ENVIRONMENT to the target environment.}"
: "${STARFORGE_DEPLOY_COMMAND:?Set STARFORGE_DEPLOY_COMMAND to the protected deployment command.}"

if [[ "${STARFORGE_DEPLOY_ENVIRONMENT}" == "production" && "${STARFORGE_DEPLOY_APPROVED:-}" != "true" ]]; then
    echo "Production deployment requires STARFORGE_DEPLOY_APPROVED=true."
    exit 1
fi

echo "Deploying to ${STARFORGE_DEPLOY_ENVIRONMENT}."
bash -o pipefail -c "${STARFORGE_DEPLOY_COMMAND}"

if [[ -n "${STARFORGE_HEALTHCHECK_URL:-}" ]]; then
    max_attempts="${STARFORGE_HEALTHCHECK_ATTEMPTS:-12}"
    interval_seconds="${STARFORGE_HEALTHCHECK_INTERVAL_SECONDS:-10}"

    for ((attempt = 1; attempt <= max_attempts; attempt++)); do
        if curl --fail --silent --show-error --max-time 10 "${STARFORGE_HEALTHCHECK_URL}" >/dev/null; then
            echo "Deployment health check passed."
            exit 0
        fi

        echo "Health check ${attempt}/${max_attempts} failed."
        sleep "${interval_seconds}"
    done

    echo "Deployment completed but did not become healthy. Run the rollback job."
    exit 1
fi

echo "Deployment command completed. No health check URL was configured."
