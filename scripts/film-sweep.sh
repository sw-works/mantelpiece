#!/usr/bin/env bash
# Delete the day-film schedules ingest made (EventBridge Scheduler, `film-*`). Each deletes
# itself after it runs; one still waiting for 21:00 would fire at a film Lambda that no
# longer exists. Schedules are not part of the CDK stack, so `make down` runs this.
set -euo pipefail
REGION="${AWS_REGION:-us-west-2}"
GROUP="${FILM_SCHEDULE_GROUP:-default}"
for name in $(aws scheduler list-schedules --region "$REGION" --group-name "$GROUP" \
  --name-prefix film- --query "Schedules[].Name" --output text); do
  aws scheduler delete-schedule --region "$REGION" --group-name "$GROUP" --name "$name"
  echo "deleted schedule $name"
done
