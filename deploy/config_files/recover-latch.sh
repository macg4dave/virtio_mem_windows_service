#!/usr/bin/env bash
# Recover a latched win11_gpu controller: stop it, abandon a stuck shrink
# (sets requested to what the guest actually kept), clear the latch with the
# installed attestation, start it again. Prompts for sudo. Install the
# current host build and attestation first (install-host.sh).
#
#   deploy/config_files/recover-latch.sh "reason recorded in the checkpoint"
set -euo pipefail
cd "$(dirname "$0")/../.."
reason=${1:?usage: recover-latch.sh REASON}
vm=win11_gpu
alias=ua-virtiomem0
unit=virtio-mem-host@$vm.service
config=/etc/virtio-mem-host/host_list.toml
attestation=/etc/virtio-mem-host/$vm.attestation.json
host=(sudo -u virtio-mem-host /usr/local/libexec/virtio-mem-host)
connect=qemu:///system

kib() {
  virsh -r -c "$connect" dumpxml "$vm" |
    xmllint --xpath "string(//memory[@model='virtio-mem'][alias/@name='$alias']/target/$1)" -
}

sudo systemctl stop "$unit"
# Start it again however recovery ends; a still-latched controller only
# observes.
trap 'sudo systemctl start "$unit"; sleep 5; systemctl status "$unit" --no-pager | head -8' EXIT
requested=$(( $(kib requested) * 1024 ))
current=$(( $(kib current) * 1024 ))
echo "requested=$requested current=$current"
if [ "$requested" != "$current" ]; then
  "${host[@]}" abandon-shrink "$vm" "$alias" "$requested" \
    --attestation "$attestation" --command-timeout-seconds 10 \
    --sample-interval-seconds 5 --convergence-timeout-seconds 60 \
    --connect "$connect" --apply
fi
"${host[@]}" clear-latch "$reason" --config "$config" --connect "$connect" --apply
