#!/bin/sh
# Entrypoint of the Docker image. When KUBECONFIG is not set (or empty) and a .kube folder is
# mounted in the home folder, every kubeconfig in it goes into KUBECONFIG, with `config` first,
# so a whole ~/.kube folder works without listing its files on the host.
set -eu

kube="$HOME/.kube"
if [ -z "${KUBECONFIG:-}" ] && [ -d "$kube" ]; then
	list="$(
		cd "$kube" &&
			{
				if [ -f config ]; then echo config; fi
				grep -rl --exclude-dir=cache --exclude-dir=http-cache '^kind: Config' . 2>/dev/null |
					sed 's|^\./||' | sort
			} | awk '!seen[$0]++' | sed "s|^|$kube/|" | paste -sd: -
	)" || list=""
	if [ -n "$list" ]; then
		export KUBECONFIG="$list"
		echo "kubri: KUBECONFIG=$KUBECONFIG" >&2
	fi
fi

exec kubri-server "$@"
