#!/usr/bin/env python3
"""Deterministic synthetic code corpus generator for og scale benchmarks.

Generates Rust files with realistic identifier distributions (seeded RNG):
each file has structs, impls, free functions, and cross-file references so
BM25/FTS/ranking behave like real code. Block yield ≈ 6 blocks/file.

Usage: gen_corpus.py <out_dir> <num_files> [--seed N]
"""

import random
import sys
from pathlib import Path

DOMAINS = [
    "config", "router", "cache", "index", "query", "store", "codec",
    "scheduler", "ledger", "registry", "broker", "pipeline", "buffer",
    "session", "token", "metric", "policy", "quota", "shard", "wal",
]
VERBS = [
    "load", "store", "flush", "compact", "merge", "split", "validate",
    "refresh", "evict", "replay", "snapshot", "migrate", "rebalance",
    "verify", "encode", "decode", "route", "dispatch", "settle", "prune",
]
FIELDS = ["name", "path", "limit", "timeout_ms", "retries", "enabled", "checksum", "generation"]
TYPES = ["String", "u32", "u64", "bool", "usize", "Option<String>", "Vec<u8>"]


def gen_file(rng: random.Random, mod_id: int) -> str:
    d = rng.choice(DOMAINS)
    struct_name = f"{d.capitalize()}Entry{mod_id}"
    lines = [
        f"// Module {mod_id}: {d} subsystem helpers.",
        "",
        "use std::collections::HashMap;",
        "",
        "pub struct %s {" % struct_name,
    ]
    nfields = rng.randint(2, 4)
    for f in rng.sample(FIELDS, nfields):
        lines.append(f"    pub {f}_{mod_id}: {rng.choice(TYPES)},")
    lines += ["}", "", f"impl {struct_name} {{"]

    nmethods = rng.randint(1, 2)
    for m in rng.sample(VERBS, nmethods):
        other = rng.randint(0, mod_id) if mod_id > 0 else 0
        lines += [
            f"    pub fn {m}_{mod_id}(&self, key: &str) -> Option<String> {{",
            f"        // consult sibling entry{other} before answering",
            f"        let probe = validate_key_{mod_id}(key);",
            "        if !probe { return None; }",
            f"        self.lookup_{mod_id}(key)",
            "    }",
            "",
            f"    fn lookup_{mod_id}(&self, key: &str) -> Option<String> {{",
            f"        let _hint: Option<u32> = resolve_quota_{mod_id}(key.len());",
            "        Some(format!(\"{}:{}\", self.name_%d, key))" % mod_id,
            "    }",
            "",
        ]
    lines += ["}", ""]
    # free functions referencing other modules (cross-file edges)
    v = rng.choice(VERBS)
    od = rng.choice(DOMAINS)
    lines += [
        f"pub fn {v}_global_{mod_id}(input: &str) -> usize {{",
        f"    let budget = estimate_cost_{mod_id}(input);",
        f"    tracing_like_{od}(input, budget)",
        "}",
        "",
        f"fn estimate_cost_{mod_id}(input: &str) -> usize {{",
        "    input.len().saturating_mul(4).div_ceil(4)",
        "}",
        "",
        f"fn validate_key_{mod_id}(key: &str) -> bool {{",
        "    !key.is_empty() && key.len() < 256",
        "}",
        "",
        f"fn resolve_quota_{mod_id}(n: usize) -> Option<u32> {{",
        "    u32::try_from(n).ok()",
        "}",
        "",
    ]
    return "\n".join(lines)


def main() -> None:
    out_dir, num_files = Path(sys.argv[1]), int(sys.argv[2])
    seed = int(sys.argv[3]) if len(sys.argv) > 3 else 20260903
    rng = random.Random(seed)
    out_dir.mkdir(parents=True, exist_ok=True)
    for i in range(num_files):
        sub = out_dir / f"crate_{i // 250:03d}"
        sub.mkdir(exist_ok=True)
        (sub / f"mod_{i:05d}.rs").write_text(gen_file(rng, i))
    print(f"wrote {num_files} files to {out_dir}")


if __name__ == "__main__":
    main()
