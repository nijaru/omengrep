#!/usr/bin/env python3
"""BM25/hybrid parity: old og (omendb) vs new og (owned storage) top-k overlap.

Both binaries index identical corpora in separate dirs (both use .og/).
Identity key: (file, name). Reports top-5/top-10 overlap per query +
means, sliced by identifier vs NL queries. og-only numbers.
Usage: parity.py <old_dir> <new_dir>
"""
import json
import subprocess
import sys

IDENT_QUERIES = [
    "vector_search",
    "rank_context",
    "write_row_bytes",
    "parse_file_reference",
    "SCHEMA_VERSION",
    "definition_weight",
    "outline_json",
]
NL_QUERIES = [
    "how to add a new block type",
    "token budget packing for context",
    "publish a generation atomically",
    "incremental rebuild after file change",
    "filter noisy symbol names",
    "fp16 vector storage",
]


def run(binary, query, path, n=10):
    out = subprocess.run(
        [binary, query, "-j", "-n", str(n), path],
        capture_output=True, text=True,
    )
    try:
        return [(r["file"], r["name"]) for r in json.loads(out.stdout)]
    except (json.JSONDecodeError, KeyError):
        return []


def overlap(a, b, k):
    sa, sb = a[:k], b[:k]
    if not sa and not sb:
        return 1.0
    return len(set(sa) & set(sb)) / k


def main():
    old_dir, new_dir = sys.argv[1], sys.argv[2]
    print(f"{'query':38s} {'top5':>6s} {'top10':>6s}")
    for label, qs in [("IDENT", IDENT_QUERIES), ("NL", NL_QUERIES)]:
        o5 = o10 = 0
        print(f"--- {label} ---")
        for q in qs:
            a = run(f"{__import__('os').environ['HOME']}/.cargo/bin/og", q, old_dir)
            b = run(f"{__import__('os').environ.get('NEW_OG', '') or 'target/release/og'}", q, new_dir)
            x5, x10 = overlap(a, b, 5), overlap(a, b, 10)
            o5 += x5
            o10 += x10
            print(f"{q:38s} {x5:6.2f} {x10:6.2f}")
        print(f"{'mean '+label:38s} {o5/len(qs):6.2f} {o10/len(qs):6.2f}")


if __name__ == "__main__":
    main()
