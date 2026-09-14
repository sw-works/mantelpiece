#!/usr/bin/env bash
# Build apps/web against the deployed stack's outputs and publish it through the stack's
# BucketDeployment. Needs AWS credentials, a deployed MemoryLane stack, and the same
# CDK_ARGS the first deploy used (custom domains, LWA client) so the template stays equal.
set -euo pipefail
cd "$(dirname "$0")/.."
STACK="${STACK_NAME:-MemoryLane}"
out() { aws cloudformation describe-stacks --stack-name "$STACK" --query "Stacks[0].Outputs[?OutputKey=='$1'].OutputValue" --output text; }
export VITE_API_BASE="$(out ApiUrl)"
export VITE_COGNITO_DOMAIN="$(out CognitoDomain)"
export VITE_COGNITO_CLIENT_ID="$(out UserPoolClientId)"
export VITE_UPLOAD_BASE="$(out UploadBase)"
echo "API $VITE_API_BASE"
echo "link $(out LinkBase)"
echo "upload $VITE_UPLOAD_BASE"
pnpm --filter @memorylane/web build
cd infra && pnpm cdk deploy --require-approval never "$@"
