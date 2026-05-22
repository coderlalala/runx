import { describe, expect, it } from "vitest";

import { createHarnessHookAdapter } from "./agent-hook.js";

type HarnessHookSource = {
  readonly type: "harness-hook";
  readonly args: readonly string[];
  readonly hook: string;
  readonly outputs?: Readonly<Record<string, unknown>>;
  readonly raw: Readonly<Record<string, unknown>>;
};

describe("harness-hook adapter", () => {
  it("invokes a deterministic hook through the adapter seam", async () => {
    const source = harnessHookSource("review-receipt", { verdict: "string" });
    const adapter = createHarnessHookAdapter({
      handlers: {
        "review-receipt": () => ({ output: { verdict: "pass" } }),
      },
    });

    const result = await adapter.invoke({
      source,
      inputs: { receipt_id: "rx_123" },
      skillDirectory: process.cwd(),
      env: process.env,
    });

    expect(result.status).toBe("sealed");
    expect(JSON.parse(result.stdout)).toEqual({ verdict: "pass" });
    expect(result.metadata).toMatchObject({
      agent_hook: {
        source_type: "harness-hook",
        hook: "review-receipt",
        status: "sealed",
      },
    });
  });

  it("returns sanitized failure metadata", async () => {
    const source = harnessHookSource("fail");
    const adapter = createHarnessHookAdapter({
      handlers: {
        fail: () => ({ status: "failure", errorMessage: "fixture failure" }),
      },
    });

    const result = await adapter.invoke({
      source,
      inputs: {},
      skillDirectory: process.cwd(),
      env: process.env,
    });

    expect(result.status).toBe("failure");
    expect(result.stderr).toBe("fixture failure");
    expect(result.metadata).toMatchObject({
      agent_hook: {
        hook: "fail",
        status: "failure",
      },
    });
  });
});

function harnessHookSource(hook: string, outputs?: Readonly<Record<string, unknown>>): HarnessHookSource {
  return {
    type: "harness-hook",
    args: [],
    hook,
    outputs,
    raw: {
      type: "harness-hook",
      hook,
      ...(outputs ? { outputs } : {}),
    },
  };
}
