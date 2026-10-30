#!/usr/bin/env bash
# Fills the cluster of the current kubeconfig context with COUNT pods in the namespace `bench`.
# The pods name a scheduler that does not exist, so they stay Pending and need no node room.
# bench/speed/seed.sh [COUNT]
set -euo pipefail

count=${1:-10000}
batch=500
dir=$(mktemp -d)
trap 'rm -rf "$dir"' EXIT

kubectl create namespace bench --dry-run=client -o yaml | kubectl apply -f - >/dev/null
# Pods are refused until the namespace's default service account exists.
for _ in $(seq 60); do kubectl get serviceaccount default -n bench >/dev/null 2>&1 && break; sleep 1; done
have=$(kubectl get pods -n bench --no-headers 2>/dev/null | wc -l)
if [ "$have" -ge "$count" ]; then
  echo "bench already has $have pods"
  exit 0
fi

python3 -I - "$dir" "$have" "$count" "$batch" <<'EOF'
import json, sys

out, start, count, batch = sys.argv[1], *map(int, sys.argv[2:])
apps = ["api", "web", "worker", "cache", "auth", "search", "emails", "billing"]
for first in range(start, count, batch):
    items = []
    for i in range(first, min(first + batch, count)):
        app = apps[i % len(apps)]
        items.append({
            "apiVersion": "v1",
            "kind": "Pod",
            "metadata": {
                "name": f"{app}-{i:05d}",
                "labels": {"app": app, "tier": "bench", "pod-template-hash": f"{i % 97:05x}"},
                "annotations": {"kubri.dev/bench": "true"},
            },
            "spec": {
                "schedulerName": "kubri-bench-none",
                "containers": [{
                    "name": app,
                    "image": f"registry.example.com/{app}:1.{i % 20}.0",
                    "ports": [{"containerPort": 8080, "name": "http"}],
                    "env": [{"name": "MODE", "value": "bench"}, {"name": "INDEX", "value": str(i)}],
                    "resources": {
                        "requests": {"cpu": "50m", "memory": "64Mi"},
                        "limits": {"memory": "128Mi"},
                    },
                }],
            },
        })
    with open(f"{out}/pods-{first:05d}.json", "w") as f:
        json.dump({"apiVersion": "v1", "kind": "List", "items": items}, f)
EOF

ls "$dir"/*.json | xargs -P 8 -I{} kubectl create -n bench -f {} -o name | wc -l | xargs echo "created pods:"
