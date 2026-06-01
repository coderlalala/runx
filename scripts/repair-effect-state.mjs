#!/usr/bin/env node
import fs from "node:fs";

function usage() {
  console.error("usage: node scripts/repair-effect-state.mjs (--strip-run-spend-ledger | --strip-finality-ledger) --path <state.json>");
  process.exit(2);
}

const args = process.argv.slice(2);
const stripRunSpendLedger = args.includes("--strip-run-spend-ledger");
const stripFinalityLedger = args.includes("--strip-finality-ledger");
const pathIndex = args.indexOf("--path");
const statePath = pathIndex >= 0 ? args[pathIndex + 1] : undefined;

if ((!stripRunSpendLedger && !stripFinalityLedger) || !statePath) usage();

const state = JSON.parse(fs.readFileSync(statePath, "utf8"));
const stripped = [];
if (state && typeof state === "object" && state.families && typeof state.families === "object") {
  for (const family of Object.values(state.families)) {
    if (family && typeof family === "object") {
      if (stripRunSpendLedger) {
        delete family.run_spend_ledger;
      }
      if (stripFinalityLedger) {
        delete family.settlement_finality;
        delete family.settlement_events;
      }
    }
  }
}
if (stripRunSpendLedger) stripped.push("run_spend_ledger");
if (stripFinalityLedger) stripped.push("settlement_finality", "settlement_events");

fs.writeFileSync(statePath, `${JSON.stringify(state, null, 2)}\n`);
console.log(JSON.stringify({ status: "repaired", path: statePath, stripped }));
