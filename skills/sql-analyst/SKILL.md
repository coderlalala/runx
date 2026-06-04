---
name: sql-analyst
description: Turn a bounded data question, schema summary, and sample rows into a reviewable SQL analysis plan.
runx:
  category: data
---

# SQL Analyst

Produce a safe, reviewable SQL analysis plan from a bounded question and enough
schema context to avoid guessing.

This skill is for read-only analysis. It should help an operator decide what to
query, how to validate it, and how to interpret the result. It does not execute
SQL, mutate data, or assume access to live databases. A consuming product or
front supplies schema summaries, sampled rows, and credentialed execution.

## Quality Profile

- Purpose: convert a data question into a precise read-only query plan.
- Audience: operators and analysts reviewing what should be queried before a
  database front executes anything.
- Artifact contract: query plan, validation checks, interpretation guidance, and
  residual risks.
- Evidence bar: tie each selected table and field to supplied schema context.
  If the schema is too thin, return `needs_schema`.
- Voice bar: concise analyst notes. Avoid generic BI advice.
- Strategic bar: make the next governed read safer and easier to review.
- Stop conditions: return `needs_schema` when required tables/fields are missing,
  and `unsafe_request` for write, delete, export-all, or broad PII requests.

## Inputs

- `question` (required): the business or product question.
- `schema_summary` (required): table and field summaries available to query.
- `sample_rows` (optional): representative non-sensitive rows.
- `constraints` (optional): limits, privacy rules, or allowed tables.

