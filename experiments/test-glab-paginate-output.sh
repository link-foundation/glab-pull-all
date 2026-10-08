#!/usr/bin/env bash
# Show the raw output format of `glab api --paginate` (issue #5).
#
# glab prints each page as a separate JSON array (`[...][...]`), which is why
# `parse_paginated_projects` streams arrays instead of using `from_str`.
# Observed with glab 1.36.0: GROUP below with per_page=2 -> 34 pages, 68
# projects, and parse_paginated_projects returned all 68.
#
# Also note: glab 1.36.0 exits with status 0 on HTTP 404 and prints the error
# body (`{"message":"404 Group Not Found"}`) to stdout.
#
# Usage: experiments/test-glab-paginate-output.sh [group] [per_page]

set -euo pipefail

GROUP="${1:-gitlab-org/developer-relations}"
PER_PAGE="${2:-2}"
ENCODED_GROUP="${GROUP//\//%2F}"
OUT="$(mktemp)"
trap 'rm -f "$OUT"' EXIT

glab api --paginate \
  "groups/${ENCODED_GROUP}/projects?per_page=${PER_PAGE}&include_subgroups=true" \
  >"$OUT"

echo "bytes:            $(wc -c <"$OUT")"
echo "page boundaries:  $(grep -o '\]\[' "$OUT" | wc -l) (pages - 1)"
echo "projects:         $(python3 -I -c '
import json, sys
s, i, d, n = sys.stdin.read(), 0, json.JSONDecoder(), 0
while (i := len(s) - len(s[i:].lstrip()) ) < len(s):
    page, i = d.raw_decode(s, i)
    n += len(page)
print(n)' <"$OUT")"
echo "first 120 chars:  $(head -c 120 "$OUT")"
