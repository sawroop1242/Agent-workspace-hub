#!/usr/bin/env python3
"""Validate LLM-produced AWH feature identifiers before they enter automation."""
import re
import sys

FEATURE_ID_RE = re.compile(r"AWH-[A-Z0-9]+(?:-[A-Z0-9]+)*\Z")


def main() -> int:
    if len(sys.argv) != 2 or not FEATURE_ID_RE.fullmatch(sys.argv[1]):
        print("Invalid feature ID: expected AWH-<TOKEN>[-<TOKEN>...], uppercase alphanumeric tokens only", file=sys.stderr)
        return 1
    print(sys.argv[1])
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
