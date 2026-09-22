#!/usr/bin/env bash
# Delete the IVS channels the API made for phone broadcasts (tag memorylane:household).
# The API deletes a channel when its broadcast ends, but IVS refuses while the stream is
# still winding down; those wait for the household's next start or stop, which may never
# come. Channels are not part of the CDK stack, so `make down` runs this. An idle channel
# costs nothing; this keeps the account tidy.
set -euo pipefail
REGION="${AWS_REGION:-us-west-2}"
for arn in $(aws ivs list-channels --region "$REGION" --query "channels[].arn" --output text); do
  owned=$(aws ivs list-tags-for-resource --region "$REGION" --resource-arn "$arn" \
    --query "tags.\"memorylane:household\"" --output text)
  [ "$owned" = "None" ] && continue
  if aws ivs delete-channel --region "$REGION" --arn "$arn" 2>/dev/null; then
    echo "deleted $arn"
  else
    echo "still busy, left $arn (run again in a minute)"
  fi
done
