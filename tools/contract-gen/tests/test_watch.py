"""Tests for the Hermes contract sync watcher workflow."""

from __future__ import annotations

import unittest
from pathlib import Path

REPO = Path(__file__).resolve().parents[3]
WORKFLOW_PATH = REPO / ".github" / "workflows" / "hermes-sync.yml"


def _load_workflow() -> str:
    return WORKFLOW_PATH.read_text(encoding="utf-8")


def _step_slice(text: str, step_id: str) -> str:
    lines = text.splitlines()
    marker = f"id: {step_id}"
    start = None
    for index, line in enumerate(lines):
        if line.strip() == marker:
            start = index
            break
    if start is None:
        raise AssertionError(f"missing step {marker!r}")
    end = len(lines)
    for index in range(start + 1, len(lines)):
        if lines[index].lstrip().startswith("- name:"):
            end = index
            break
    return "\n".join(lines[start:end])


class WatcherWorkflowTest(unittest.TestCase):
    def test_stops_before_bump_when_nothing_is_new(self) -> None:
        text = _load_workflow()
        compare = _step_slice(text, "compare")
        for expected in (
            "resolve_tag",
            "read_pin",
            "compare_versions",
            "gh issue list --state open --search",
            "in:title",
            "--limit 100",
            "--json title,number",
            "git ls-remote --exit-code --heads origin",
            "is ready to sync",
            "needs a manual sync",
            "id: compare",
            "chore/hermes-sync-",
        ):
            self.assertIn(expected, compare)
        for forbidden in (
            "bump",
            "gh pr",
            "rustup toolchain install",
            "cargo install",
        ):
            self.assertNotIn(forbidden, compare)
        for expected in (
            "workflow_dispatch:",
            "schedule:",
            'cron: "23 7 * * *"',
            "steps.compare.outputs.proceed == 'true'",
        ):
            self.assertIn(expected, text)
        self.assertFalse(
            any(line.lstrip().startswith("concurrency:") for line in text.splitlines()),
            "workflow must not define concurrency:",
        )

    def test_success_pushes_a_branch_and_opens_one_issue(self) -> None:
        text = _load_workflow()
        bump = _step_slice(text, "bump")
        test = _step_slice(text, "test")
        pull = _step_slice(text, "pull")
        combined = "\n".join([bump, test, pull])
        for expected in (
            "python3 tools/contract-gen/contract_gen.py bump --tag",
            "cargo test --workspace",
            "crates/hermes-contract/PINNED",
            "crates/hermes-contract/vendor/gateway-contract.openrpc.json",
            "crates/hermes-contract/vendor/gateway-contract.openrpc.json.sha256",
            "crates/hermes-contract/vendor/LICENSE.hermes",
            "crates/hermes-contract/src/generated/",
            "crates/hermes-contract/compat.toml",
            "docs/compatibility.md",
            "README.md",
            'git config user.name "github-actions[bot]"',
            'git config user.email "41898282+github-actions[bot]@users.noreply.github.com"',
            "git switch -c",
            "chore(contract): sync with Hermes",
            "git push origin HEAD",
            "chore/hermes-sync-",
            "quick_pull=1",
            "Sync+with+Hermes+",
            "both commands passed",
            "Closes",
            '--body-file "$RUNNER_TEMP/hermes-sync-ready.txt"',
            "$RUNNER_TEMP/hermes-sync-bump.txt",
            "$RUNNER_TEMP/hermes-sync-bump-err.txt",
            "$RUNNER_TEMP/hermes-sync-test.txt",
            "gh issue create",
            "is ready to sync",
        ):
            self.assertIn(expected, combined)
        for forbidden in (
            "recorded_against",
            "gh pr",
            "--force",
            "git tag",
            "gh release create",
            "<tag>",
        ):
            self.assertNotIn(forbidden, pull)
        commit_lines = [
            line for line in pull.splitlines() if "git commit" in line
        ]
        self.assertEqual(len(commit_lines), 1, pull)
        self.assertIn("chore(contract): sync with Hermes", commit_lines[0])
        self.assertEqual(commit_lines[0].count(" -m "), 1, commit_lines[0])
        self.assertNotIn("Co-authored-by:", pull)
        self.assertNotIn("Signed-off-by:", pull)

    def test_failure_opens_or_updates_one_issue(self) -> None:
        text = _load_workflow()
        issue = _step_slice(text, "issue")
        for expected in (
            "failure()",
            "steps.bump.outcome",
            "steps.test.outcome",
            "gh issue create",
            "gh issue edit",
            "needs a manual sync",
            "contract_gen.py bump --tag",
            "cargo test --workspace",
            '--body-file "$RUNNER_TEMP/hermes-sync-failure.txt"',
            "tail -n 100",
            "50000",
        ):
            self.assertIn(expected, issue)
        for forbidden in ("gh pr", "<tag>"):
            self.assertNotIn(forbidden, issue)

    def test_permissions_use_only_the_repository_token(self) -> None:
        text = _load_workflow()
        for expected in (
            "contents: write",
            "issues: write",
            "${{ github.token }}",
            "actions/checkout@fbc6f3992d24b796d5a048ff273f7fcc4a7b6c09",
            "rustup toolchain install",
            "contract-check-debug",
            "typify-install-release",
            "cargo install cargo-typify --version 0.8.0 --locked",
            "CARGO_TARGET_DIR",
            "CARGO_BUILD_BUILD_DIR",
        ):
            self.assertIn(expected, text)
        for forbidden in (
            "pull-requests",
            "secrets.",
            "git clone",
            "--hermes-checkout",
            "contract-drift",
            "<tag>",
        ):
            self.assertNotIn(forbidden, text)


class ScheduledDriftReportTest(unittest.TestCase):
    def test_a_newer_hermes_release_never_turns_the_report_red(self) -> None:
        text = (REPO / ".github" / "workflows" / "contract.yml").read_text(encoding="utf-8")
        start = text.index("      - name: Report drift")
        step = text[start:]
        self.assertIn("continue-on-error: true", step)
        self.assertIn("contract_gen.py drift --tag latest", step)


if __name__ == "__main__":
    unittest.main()
