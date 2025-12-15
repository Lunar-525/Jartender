#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
cd "$SCRIPT_DIR"

files=(list config java_list)

for base in "${files[@]}"; do
  json="${base}.json"
  dis="${base}.json.dis"

  if [ -f "$json" ]; then
    mv -f "$json" "$dis"
    echo "切换 $json -> $dis"
  elif [ -f "$dis" ]; then
    mv -f "$dis" "$json"
    echo "切换 $dis -> $json"
  else
    echo "跳过 因为 $json / $dis 均不存在"
  fi
done
