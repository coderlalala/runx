#!/usr/bin/env python3
import json
import os
import shutil
import subprocess
import tempfile
from pathlib import Path


def main() -> None:
    scafld = os.environ.get("RUNX_INPUT_SCAFLD_BIN") or os.environ.get("SCAFLD_BIN") or "scafld"
    task_id = os.environ.get("RUNX_INPUT_TASK_ID") or "hosted-scafld-smoke"
    title = os.environ.get("RUNX_INPUT_TITLE") or "Hosted scafld smoke"
    root = Path(tempfile.mkdtemp(prefix="runx-hosted-scafld-smoke-"))
    steps = []
    try:
        (root / "README.md").write_text("# hosted scafld smoke\n", encoding="utf-8")
        version = run([scafld, "--version"], root, steps).stdout.strip()
        init = run_scafld(scafld, ["init", "--json"], root, steps)
        plan = run_scafld(
            scafld,
            [
                "plan",
                task_id,
                "--title",
                title,
                "--summary",
                "Hosted runx smoke for the pinned scafld release.",
                "--size",
                "micro",
                "--risk",
                "low",
                "--command",
                "test -f README.md",
                "--json",
            ],
            root,
            steps,
        )
        validate = run_scafld(scafld, ["validate", task_id, "--json"], root, steps)
        approve = run_scafld(scafld, ["approve", task_id, "--json"], root, steps)
        build = run_scafld(scafld, ["build", task_id, "--json"], root, steps)
        status = run_scafld(scafld, ["status", task_id, "--json"], root, steps)
        handoff = run_scafld(scafld, ["handoff", task_id], root, steps)

        print(
            json.dumps(
                {
                    "ok": True,
                    "scafld_version": version,
                    "task_id": task_id,
                    "init": json.loads(init.stdout),
                    "plan": json.loads(plan.stdout),
                    "validate": json.loads(validate.stdout),
                    "approve": json.loads(approve.stdout),
                    "build": json.loads(build.stdout),
                    "status": json.loads(status.stdout),
                    "handoff": handoff.stdout,
                    "steps": steps,
                },
                separators=(",", ":"),
            )
        )
    finally:
        shutil.rmtree(root, ignore_errors=True)


def run_scafld(scafld: str, args: list[str], cwd: Path, steps: list[dict[str, object]]):
    return run([scafld, *args], cwd, steps)


def run(command: list[str], cwd: Path, steps: list[dict[str, object]]):
    result = subprocess.run(
        command,
        cwd=cwd,
        env=sanitized_env(),
        text=True,
        capture_output=True,
        check=False,
    )
    steps.append({"command": command[1:], "exit_code": result.returncode})
    if result.returncode != 0:
        raise RuntimeError((result.stderr or result.stdout or f"{command[0]} failed").strip())
    return result


def sanitized_env() -> dict[str, str]:
    keep = ("PATH", "HOME", "TMPDIR", "TMP", "TEMP", "SystemRoot", "WINDIR", "COMSPEC", "PATHEXT")
    return {key: value for key, value in os.environ.items() if key in keep}


if __name__ == "__main__":
    main()
