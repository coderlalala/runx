import { describe, expect, it } from "vitest";

import { rewriteSiblingSkillRefs } from "../packages/cli/src/skill-refs.js";

describe("rewriteSiblingSkillRefs", () => {
  it("rewrites ../sibling refs to canonical ids", () => {
    const text = `runners:
  default:
    type: graph
    graph:
      steps:
        - id: a
          skill: ../research
        - id: b
          skill: ../leaf
`;
    const versions = new Map([["research", "sha-1"], ["leaf", "sha-2"]]);
    const result = rewriteSiblingSkillRefs(text, "runx", versions);
    expect(result.didRewrite).toBe(true);
    expect(result.text).toContain("skill: runx/research@sha-1");
    expect(result.text).toContain("skill: runx/leaf@sha-2");
    expect(result.text).not.toContain("skill: ../research");
    expect(result.text).not.toContain("skill: ../leaf");
  });

  it("leaves unknown siblings untouched", () => {
    const text = "skill: ../missing\nskill: ../known\n";
    const versions = new Map([["known", "sha-1"]]);
    const result = rewriteSiblingSkillRefs(text, "runx", versions);
    expect(result.didRewrite).toBe(true);
    expect(result.text).toContain("skill: ../missing");
    expect(result.text).toContain("skill: runx/known@sha-1");
  });

  it("is a no-op when no refs match", () => {
    const text = "skill: runx/already-canonical@v1\n";
    const result = rewriteSiblingSkillRefs(text, "runx", new Map([["foo", "v"]]));
    expect(result.didRewrite).toBe(false);
    expect(result.text).toBe(text);
  });
});
