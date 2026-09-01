#!/bin/bash
# Apply the registered campaign environment controls on atlas (section 3).
# Reversible: state saved under /var/tmp/opb-prev; undo with atlas_restore.sh.
# Requires root (run via sudo).
set -u

STATE=/var/tmp/opb-prev
mkdir -p "$STATE/irq"

echo "== saving current state to $STATE =="
cat /proc/sys/kernel/numa_balancing > "$STATE/numa_balancing"
cat /sys/kernel/mm/transparent_hugepage/enabled > "$STATE/thp"
systemctl is-active irqbalance > "$STATE/irqbalance" || true
cat /proc/irq/default_smp_affinity > "$STATE/irq_default"
for d in /proc/irq/[0-9]*; do
  n=$(basename "$d")
  cat "$d/smp_affinity_list" > "$STATE/irq/$n" 2>/dev/null || true
done

echo "== kernel.numa_balancing=0 (registered) =="
sysctl -w kernel.numa_balancing=0

echo "== THP madvise (registered; shmem stays never) =="
echo madvise > /sys/kernel/mm/transparent_hugepage/enabled

echo "== irqbalance stop =="
systemctl stop irqbalance

echo "== IRQ affinity -> CPUs 60-63 (sacrificial core 15, node 8) =="
# default_smp_affinity: 32-bit words, rightmost = CPUs 0-31; 144 CPUs = 5
# words; CPUs 60-63 = top nibble of the second word from the right.
echo "00000000,00000000,00000000,f0000000,00000000" > /proc/irq/default_smp_affinity
moved=0; failed=0
for d in /proc/irq/[0-9]*; do
  if echo "60-63" > "$d/smp_affinity_list" 2>/dev/null; then
    moved=$((moved+1))
  else
    failed=$((failed+1))
    basename "$d" >> "$STATE/irq_unmovable"
  fi
done
echo "moved $moved IRQs; $failed unmovable (recorded)"

echo "== system.slice corralled to CPUs 60-63 (runtime property) =="
systemctl set-property --runtime system.slice AllowedCPUs=60-63

echo "== done; verify =="
echo "numa_balancing: $(cat /proc/sys/kernel/numa_balancing)"
echo "thp: $(cat /sys/kernel/mm/transparent_hugepage/enabled)"
echo "irqbalance: $(systemctl is-active irqbalance || true)"
