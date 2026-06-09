export type SandboxProfile = "readonly" | "workspace-write" | "network" | "unrestricted-local-dev";

export interface SandboxDeclaration {
  readonly profile: SandboxProfile;
  readonly cwdPolicy?: "skill-directory" | "workspace" | "custom";
  readonly envAllowlist?: readonly string[];
  readonly network?: boolean;
  readonly writablePaths?: readonly string[];
  readonly requireEnforcement?: boolean;
}

export interface RequiredSandboxDeclaration {
  readonly profile: SandboxProfile;
  readonly cwdPolicy: "skill-directory" | "workspace" | "custom";
  readonly envAllowlist?: readonly string[];
  readonly network: boolean;
  readonly writablePaths: readonly string[];
  readonly requireEnforcement: boolean;
}

export function normalizeSandboxDeclaration(sandbox: SandboxDeclaration | undefined): RequiredSandboxDeclaration {
  return {
    profile: sandbox?.profile ?? "readonly",
    cwdPolicy: sandbox?.cwdPolicy ?? "skill-directory",
    envAllowlist: sandbox?.envAllowlist,
    network: sandbox?.network ?? sandbox?.profile === "network",
    writablePaths: sandbox?.writablePaths ?? [],
    requireEnforcement: sandbox?.requireEnforcement ?? sandbox?.profile !== "unrestricted-local-dev",
  };
}
