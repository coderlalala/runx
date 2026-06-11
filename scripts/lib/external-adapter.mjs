// Shared host for runx external adapters (runx.external_adapter.v1).
//
// An external adapter is a subprocess: the runtime writes one invocation frame to
// stdin and reads one response frame from stdout. This module owns that protocol
// once, so adapter files only implement their domain logic.
export function runAdapter(handler) {
  let input = "";
  process.stdin.on("data", (chunk) => {
    input += chunk;
  });
  process.stdin.on("end", async () => {
    let invocation = {};
    try {
      invocation = JSON.parse(input.trim() || "{}");
    } catch {
      invocation = {};
    }
    const frame = (status, output, stderr) =>
      JSON.stringify({
        schema: "runx.external_adapter.response.v1",
        protocol_version: "runx.external_adapter.v1",
        adapter_id: invocation.adapter_id,
        invocation_id: invocation.invocation_id,
        status,
        exit_code: status === "completed" ? 0 : 1,
        observed_at: "2026-06-02T00:00:00Z",
        stdout: JSON.stringify(output),
        stderr: stderr ?? "",
        output,
        artifacts: [],
        telemetry: [],
      });
    const inputs = { ...(invocation.inputs || {}), ...(invocation.resolved_inputs || {}) };
    try {
      const output = await handler({ inputs, invocation });
      process.stdout.write(frame("completed", output ?? {}));
    } catch (error) {
      const message = error && error.message ? error.message : String(error);
      process.stdout.write(frame("failed", { error: message }, message));
    }
  });
}
