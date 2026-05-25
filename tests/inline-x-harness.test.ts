import { describe, expect, it } from "vitest";

import { createDefaultSkillAdapters } from "@runxhq/adapters";
import { runHarnessTarget } from "@runxhq/runtime-local/harness";
import { kernelEnv } from "./runx-binary.js";

describe("inline x harness", () => {
  it("runs the evolve inline harness suite successfully", async () => {
    const result = await runHarnessTarget("skills/evolve", {
      adapters: createDefaultSkillAdapters(),
      env: kernelEnv(),
    });

    expect(result.source).toBe("inline");
    if (!("cases" in result)) {
      throw new Error("expected inline harness suite");
    }
    expect(result.status).toBe("sealed");
    expect(result.assertionErrors).toEqual([]);
    expect(result.cases.map((entry) => entry.fixture.name)).toEqual(["evolve-introspect", "evolve-plan-spec"]);
    expect(result.cases[0]?.receipt).toMatchObject({ schema: "runx.receipt.v1" });
    expect(result.cases[1]?.receipt).toMatchObject({ schema: "runx.receipt.v1" });
  }, 15_000);

  it("runs the Sourcey inline harness suite through the skill package", async () => {
    const result = await runHarnessTarget("skills/sourcey", {
      adapters: createDefaultSkillAdapters(),
      env: kernelEnv(),
    });

    expect(result.source).toBe("inline");
    if (!("cases" in result)) {
      throw new Error("expected inline harness suite");
    }
    expect(result.status).toBe("sealed");
    expect(result.assertionErrors).toEqual([]);
    expect(result.cases.map((entry) => entry.fixture.name)).toEqual([
      "sourcey-discovery-yield",
      "sourcey-needs-project-input",
    ]);
    expect(result.cases[0]?.status).toBe("needs_agent");
    expect(result.cases[1]?.status).toBe("needs_agent");
  });
});
