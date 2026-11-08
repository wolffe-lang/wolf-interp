#!/usr/bin/env bash
# s288: H2's package temperature in whole degrees C (coretemp's
# temp1_input, the reading H2-env/bin/light gates on), or 0.
for z in /sys/class/hwmon/hwmon*; do
  if [ "$(cat "$z/name" 2>/dev/null)" = coretemp ]; then
    echo $(( $(cat "$z/temp1_input") / 1000 ))
    exit 0
  fi
done
echo 0
