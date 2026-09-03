# Agent task-level A/B protocol (Semble-style) — STATUS: SPECIFIED, NOT RUN

Required before any public quality claim (project policy). Retrieval
qrels (`bench/qrels.json`) measure ranking; this measures agent utility:
does og reduce the work an agent does to answer code questions?

## Design

- **Corpus**: `bench/fixtures/qrels-corpus/` (frozen, committed).
- **Conditions** (same tasks, same agent, same model):
  - A: agent with `og context`/`og search --json` available.
  - B: agent with ripgrep only (lexical baseline).
- **Tasks** (6–10, each with a checkable answer): e.g. "Where is the
  token budget enforced and what guarantees non-empty output?", "What
  forces a full rebuild on upgrade?", "Which blocks reference
  `SCHEMA_VERSION` and which defines it?" Each task ships a grading
  rubric (files/symbols the answer must cite).
- **Metrics per task**: tool calls (count), input tokens (sum over
  calls), wall time, answer grade (pass/fail vs rubric).
- **Harness sketch**: script feeds each task prompt to `pi` non-
  interactively per condition, captures the transcript, extracts tool
  calls + token usage from the session log, grades by rubric (human or
  judge model — record which). Randomize condition order per task.
- **Decision rule**: ship claims only if A wins on grade-adjusted cost
  (tokens × time per passing answer) with all raw transcripts published
  alongside, og-only framing per project policy.

## Why not run yet

12+ agent runs with instrumentation that doesn't exist yet; retrieval
gates (scale, parity, qrels) all pass and are the prerequisite signal.
Next step: build the transcript-capture harness, pilot 2 tasks × 2
conditions, then scale to the full set.
