#!/usr/bin/env bash
# Makes the demo snapshot ui/static/demo.kubri from a cluster you may change, such as k3s:
# applies scripts/demo/shop.yaml, rolls the frontend out once more so it has a history, waits
# for logs and restarts, and saves the namespace `shop` with kubri-server.
#
#   scripts/demo/make.sh            # KUBECONFIG names the cluster
#
# KUBRI_BIN (target/release/kubri-server) and KUBRI_UI_DIR (ui/build) as for the server.
set -euo pipefail

root=$(cd "$(dirname "$0")/../.." && pwd)
work=$(mktemp -d)
trap 'kill "${server:-0}" 2>/dev/null || true; rm -rf "$work"' EXIT

# The snapshot names its context: call it `demo`.
cp "$(echo "${KUBECONFIG:-$HOME/.kube/config}" | cut -d: -f1)" "$work/kubeconfig"
export KUBECONFIG="$work/kubeconfig"
kubectl config rename-context "$(kubectl config current-context)" demo >/dev/null

kubectl apply -f "$root/scripts/demo/shop.yaml"
kubectl rollout status deployment/frontend deployment/api statefulset/redis -n shop --timeout=180s
if [ "$(kubectl get deployment frontend -n shop -o jsonpath='{.metadata.annotations.deployment\.kubernetes\.io/revision}')" = 1 ]; then
  kubectl set image deployment/frontend frontend=busybox:1.37 -n shop
  kubectl annotate deployment/frontend -n shop --overwrite kubernetes.io/change-cause="frontend 2.5.0: faster product pages"
  kubectl rollout status deployment/frontend -n shop --timeout=180s
fi
# A Helm release with two revisions, when helm is there.
if command -v helm >/dev/null; then
  helm upgrade --install search "$root/scripts/demo/search" -n shop --wait
  helm upgrade search "$root/scripts/demo/search" -n shop --set indexSize=large --set replicas=2 --wait
fi
# Logs to show, a few restarts of emails, and runs of the CronJob.
echo "waiting ${DEMO_WAIT:-240} s for logs and restarts"
sleep "${DEMO_WAIT:-240}"

mkdir -p "$work/home/k9s"
KUBRI_CONFIG_DIR="$work/home/kubri" K9S_CONFIG_DIR="$work/home/k9s" \
  KUBRI_UI_DIR="${KUBRI_UI_DIR:-$root/ui/build}" \
  "${KUBRI_BIN:-$root/target/release/kubri-server}" --addr 127.0.0.1:7899 >"$work/server.log" 2>&1 &
server=$!
for _ in $(seq 100); do curl -sf http://127.0.0.1:7899/ >/dev/null && break; sleep 0.1; done
curl -sf -X PUT -H 'content-type: application/json' -d '{"name":"demo"}' http://127.0.0.1:7899/api/context >/dev/null
curl -sf -X POST -o "$root/ui/static/demo.kubri" 'http://127.0.0.1:7899/api/snapshot?namespace=shop'
echo "wrote ui/static/demo.kubri ($(wc -c <"$root/ui/static/demo.kubri") bytes)"
