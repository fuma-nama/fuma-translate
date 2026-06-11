#!/usr/bin/env bash
set -euo pipefail

root="$(cd "$(dirname "$0")/../.." && pwd)"
fixture="$root/test/fixtures/basic.tsx"

basic_dir="/tmp/fuma-translate-bench/files"
large_dir="/tmp/fuma-translate-bench-large/files"

rm -rf /tmp/fuma-translate-bench /tmp/fuma-translate-bench-large
mkdir -p "$basic_dir" "$large_dir"

echo "Generating 5000 basic files..."
for i in $(seq 1 5000); do
  cp "$fixture" "$basic_dir/file-$i.tsx"
done

echo "Generating 5000 large files..."
for i in $(seq 1 5000); do
  {
    echo 'import { useTranslations } from "@fuma-translate/react";'
    echo
    echo "export function Page$i() {"
    echo '  const t = useTranslations({ note: "page '"$i"'" });'
    echo "  return ("
    echo "    <>"
    for j in $(seq 1 50); do
      echo "      {t(\"Key $j\", { note: \"section $j\" })}"
    done
    echo "    </>"
    echo "  );"
    echo "}"
  } >"$large_dir/file-$i.tsx"
done

echo "Done."
echo "Run: cargo test --release bench -- --ignored --nocapture"
