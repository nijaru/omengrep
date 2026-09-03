#!/usr/bin/env python3
"""Retrieval quality eval: qrels MRR + Recall@k per engine config.

Each config indexes an identical corpus copy (old and new og share .og/).
Identity key: (file, name). Usage:
  eval.py qrels.json name:binary:corpus[:extra-args...] [...]
Example:
  eval.py bench/qrels.json hybrid-new:target/release/og:/tmp/qrels-new \\
      bm25-new:target/release/og:/tmp/qrels-new:--no-semantic \\
      hybrid-old:$HOME/.cargo/bin/og:/tmp/qrels-old
"""
import json
import subprocess
import sys


def run(binary, corpus, query, extra, n=10):
    cmd = [binary, *extra, query, "-j", "-n", str(n), corpus]
    try:
        out = subprocess.run(cmd, capture_output=True, text=True, timeout=120)
        return [(r["file"], r["name"]) for r in json.loads(out.stdout)]
    except Exception:
        return []


def rr(hits, relevant):
    rel = set(map(tuple, relevant))
    for i, h in enumerate(hits, 1):
        if tuple(h) in rel:
            return 1.0 / i
    return 0.0


def recall(hits, relevant, k):
    rel = set(map(tuple, relevant))
    if not rel:
        return 1.0
    return len(set(map(tuple, hits[:k])) & rel) / len(rel)


def main():
    qrels = json.load(open(sys.argv[1]))["queries"]
    for spec in sys.argv[2:]:
        parts = spec.split(":")
        name, binary, corpus, extra = parts[0], parts[1], parts[2], parts[3:]
        agg = {"all": [], "ident": [], "nl": []}
        for item in qrels:
            hits = run(binary, corpus, item["q"], extra)
            m = (rr(hits, item["relevant"]), recall(hits, item["relevant"], 5),
                 recall(hits, item["relevant"], 10))
            agg["all"].append(m)
            agg[item["kind"]].append(m)
        print(f"## {name}")
        for slice_name, ms in agg.items():
            n = len(ms)
            mrr = sum(m[0] for m in ms) / n
            r5 = sum(m[1] for m in ms) / n
            r10 = sum(m[2] for m in ms) / n
            print(f"  {slice_name:6s} n={n:3d}  MRR={mrr:.3f}  R@5={r5:.3f}  R@10={r10:.3f}")


if __name__ == "__main__":
    main()
