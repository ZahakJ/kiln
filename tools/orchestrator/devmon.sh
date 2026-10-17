#!/bin/bash
# Device health watch: prints a line only when something needs attention.
# Auto-cleans stale Rust build output when disk gets low.
SP=/tmp/claude-1000/-home-muth/d715ab71-9a3e-44c3-93cc-3ddf94f05d63/scratchpad
declare -A last
say() { local now=$(date +%s); if (( now - ${last[$1]:-0} >= 600 )); then last[$1]=$now; echo "$(date +%H:%M) $2"; fi; }
thr() { cat /sys/devices/system/cpu/cpu*/thermal_throttle/core_throttle_count 2>/dev/null | awk '{s+=$1} END{print s+0}'; }
prev=$(thr)
while true; do
  free_g=$(df -BG --output=avail / | tail -1 | tr -dc 0-9)
  if (( free_g < 20 )); then
    for t in ~/OfficeSuite/target ~/wp*-tree/target ~/OfficeSuite-wp*/target "$SP"/*target* "$SP"/tgt; do
      find "$t" -mindepth 3 -maxdepth 3 -path '*/incremental/*' -type d -mmin +30 -exec rm -rf {} + 2>/dev/null
      find "$t" \( -path '*/deps/*' -o -path '*/examples/*' -o -path '*/build/*' \) -type f -mmin +60 -delete 2>/dev/null
    done
    find ~/.cache/sccache -type f -mmin +720 -delete 2>/dev/null
    after=$(df -BG --output=avail / | tail -1 | tr -dc 0-9)
    (( after < 10 )) && say disk "DISK low even after auto-clean: ${after}G free (was ${free_g}G)"
  fi
  temp=$(( $(cat /sys/class/thermal/thermal_zone*/temp 2>/dev/null | sort -n | tail -1) / 1000 ))
  (( temp >= 97 )) && say temp "HOT: CPU package ${temp}C"
  cur=$(thr); rate=$(( (cur - prev) )); prev=$cur
  (( rate > 3000 )) && say throttle "THROTTLING: ${rate} thermal throttle events in last 60s (pkg ${temp}C, load $(cut -d' ' -f1 /proc/loadavg))"
  avail_mb=$(awk '/MemAvailable/{print int($2/1024)}' /proc/meminfo)
  swap_used=$(awk '/SwapTotal/{t=$2}/SwapFree/{f=$2}END{print int((t-f)/1048576)}' /proc/meminfo)
  (( avail_mb < 600 )) && say mem "MEMORY low: ${avail_mb}MB available, swap used ${swap_used}G"
  (( swap_used >= 7 )) && say swap "SWAP heavy: ${swap_used}G used"
  sleep 60
done
