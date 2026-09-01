#!/bin/bash
# Undo atlas_setup.sh from the state saved under /var/tmp/opb-prev.
# Requires root (run via sudo).
set -u

STATE=/var/tmp/opb-prev
[ -d "$STATE" ] || { echo "no saved state at $STATE"; exit 1; }

echo "== restore numa_balancing =="
sysctl -w kernel.numa_balancing="$(cat "$STATE/numa_balancing")"

echo "== restore THP =="
prev_thp=$(tr -d '[]' <<< "$(grep -o '\[.*\]' "$STATE/thp")")
echo "${prev_thp:-always}" > /sys/kernel/mm/transparent_hugepage/enabled

echo "== restore IRQ affinities =="
cat "$STATE/irq_default" > /proc/irq/default_smp_affinity
for f in "$STATE"/irq/*; do
  n=$(basename "$f")
  cat "$f" > "/proc/irq/$n/smp_affinity_list" 2>/dev/null || true
done

echo "== un-corral system.slice =="
systemctl set-property --runtime system.slice AllowedCPUs=""

echo "== restore swap =="
swapon -a || true

echo "== restart irqbalance if it was active =="
if grep -q active "$STATE/irqbalance" 2>/dev/null; then
  systemctl start irqbalance
fi

echo "== done; verify =="
echo "numa_balancing: $(cat /proc/sys/kernel/numa_balancing)"
echo "thp: $(cat /sys/kernel/mm/transparent_hugepage/enabled)"
echo "irqbalance: $(systemctl is-active irqbalance || true)"
