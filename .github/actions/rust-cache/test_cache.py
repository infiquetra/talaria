"""Offline cache routing and trust-boundary checks; no Cargo work or cache service."""

import re
import tempfile
import unittest
from pathlib import Path

import cache


class CachePolicyTests(unittest.TestCase):
    def setUp(self) -> None:
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        (self.root / "Cargo.lock").write_text("locked dependencies\n")
        (self.root / "Cargo.toml").write_text("[workspace]\n")
        (self.root / "rust-toolchain.toml").write_text(
            '[toolchain]\nchannel = "1.98.1"\n'
        )
        self.env = {
            "RUNNER_OS": "Linux",
            "RUNNER_ARCH": "X64",
            "GITHUB_EVENT_NAME": "push",
            "GITHUB_REF": "refs/heads/main",
            "DEFAULT_BRANCH": "main",
        }
        self.compiler = (
            "rustc 1.98.1\nrelease: 1.98.1\nhost: x86_64-unknown-linux-gnu\n"
        )

    def plan(self, role: str = "native-debug", **changes: str) -> dict[str, str]:
        return cache.plan(
            self.root, role, self.env | changes, self.compiler, "cargo 1.98.1"
        )

    def test_identical_inputs_reuse_only_the_same_build_role(self) -> None:
        native = self.plan()
        self.assertEqual(native, self.plan())
        for role in cache.BUILD_PATHS:
            with self.subTest(role=role):
                other = self.plan(role)
                self.assertEqual(native["download-key"], other["download-key"])
                if role != "native-debug":
                    self.assertNotEqual(native["build-key"], other["build-key"])
        self.assertEqual(
            self.plan("coverage-debug")["build-path"], "target/llvm-cov-target/debug"
        )
        self.assertEqual(self.plan("bench-release")["build-path"], "target/release")
        self.assertEqual(self.plan("contract-drift")["build-path"], "")

    def test_stale_platform_lock_toolchain_and_configuration_cannot_hit(self) -> None:
        original = self.plan()
        variants = [self.plan(RUNNER_OS="macOS"), self.plan(RUNNER_ARCH="ARM64")]
        variants.append(
            cache.plan(
                self.root,
                "native-debug",
                self.env,
                self.compiler + "commit-hash: changed\n",
                "cargo 1.98.1",
            )
        )
        variants.append(self.plan(RUSTFLAGS="-C target-cpu=native"))
        for name in ("Cargo.lock", "Cargo.toml", "rust-toolchain.toml"):
            path = self.root / name
            before = path.read_text()
            path.write_text(before + "# changed\n")
            variants.append(self.plan())
            path.write_text(before)
        for changed in variants:
            for key in ("download-key", "build-key"):
                self.assertNotEqual(original[key], changed[key])
                self.assertFalse(changed[key].startswith(original[key]))

    def test_missing_lock_wrong_compiler_and_unknown_role_disable_planning(
        self,
    ) -> None:
        with self.assertRaises(ValueError):
            self.plan("unknown")
        with self.assertRaises(ValueError):
            cache.plan(
                self.root,
                "native-debug",
                self.env,
                self.compiler.replace("1.98.1", "1.98.0"),
                "cargo 1.98.1",
            )
        (self.root / "Cargo.lock").unlink()
        with self.assertRaises(FileNotFoundError):
            self.plan()

    def test_only_default_branch_trusted_events_write_shared_caches(self) -> None:
        for event in ("push", "schedule", "workflow_dispatch"):
            self.assertEqual(
                self.plan(GITHUB_EVENT_NAME=event)["save-downloads"], "true"
            )
        for event in ("pull_request", "pull_request_target", "workflow_run"):
            for ref in ("refs/pull/42/merge", "refs/heads/main"):
                for role in cache.BUILD_PATHS:
                    with self.subTest(event=event, ref=ref, role=role):
                        result = self.plan(
                            role, GITHUB_EVENT_NAME=event, GITHUB_REF=ref
                        )
                        self.assertEqual(result["save-downloads"], "false")
                        self.assertEqual(result["save-build"], "false")
        for ref in ("refs/heads/topic", "refs/tags/v1.2.3"):
            self.assertEqual(self.plan(GITHUB_REF=ref)["save-downloads"], "false")
        self.assertEqual(self.plan(DEFAULT_BRANCH="")["save-downloads"], "false")

    def test_release_saves_stay_within_trusted_tag_or_default_branch(self) -> None:
        tagged = self.plan("release-debug", GITHUB_REF="refs/tags/v1.2.3-rc1")
        self.assertEqual(tagged["save-build"], "true")
        self.assertEqual(tagged["save-downloads"], "false")
        self.assertEqual(
            self.plan("native-debug", GITHUB_REF="refs/tags/v1.2.3")["save-build"],
            "false",
        )
        for ref in ("refs/tags/v1.2.3;bad", "refs/heads/topic", "refs/pull/42/merge"):
            self.assertEqual(
                self.plan("release-debug", GITHUB_REF=ref)["save-build"], "false"
            )
        self.assertNotEqual(
            tagged["build-key"], self.plan("coverage-debug")["build-key"]
        )


class WorkflowPolicyTests(unittest.TestCase):
    @staticmethod
    def check_cache_steps(source: str, indent: int, composite: bool = False) -> None:
        blocks = re.split(r"(?m)^" + " " * indent + r"- ", source)[1:]
        uses = 0
        for block in blocks:
            if (
                "uses: actions/cache" not in block
                and "uses: ./.github/actions/rust-cache" not in block
            ):
                continue
            uses += 1
            if not re.search(r"(?m)^ +continue-on-error: true$", block):
                raise ValueError("cache failure could block original work")
            if composite:
                authority = (
                    "save-downloads" if "download-key" in block else "save-build"
                )
                operator = "!=" if "uses: actions/cache/restore@" in block else "=="
                if f"steps.plan.outputs.{authority} {operator} 'true'" not in block:
                    raise ValueError(
                        "cache writer or reader has lost its authority guard"
                    )
        if not uses:
            raise ValueError("missing cache integration")

    def test_all_roles_use_optional_caches_without_gating_original_work(self) -> None:
        root = Path(__file__).resolve().parents[3]
        roles = {
            "ci.yml": ("native-debug", "coverage-debug"),
            "bench.yml": ("bench-release",),
            "contract.yml": ("contract-check-debug", "contract-drift"),
            "release.yml": ("release-debug",),
        }
        for name, expected in roles.items():
            source = (root / ".github/workflows" / name).read_text()
            with self.subTest(workflow=name):
                self.check_cache_steps(source, 6)
                self.assertEqual(
                    source.count("uses: ./.github/actions/rust-cache"), len(expected)
                )
                for role in expected:
                    self.assertIn("role: " + role + "\n", source)
                self.assertNotIn("cache-hit", source)
        action = (Path(__file__).parent / "action.yml").read_text()
        self.check_cache_steps(action, 4, composite=True)
        self.assertNotIn("restore-keys:", action)
        self.assertNotIn("fail-on-cache-miss:", action)
        self.assertNotIn("/.cargo/bin", action)
        self.assertNotIn("target/criterion", action)
        self.assertNotIn("save-always:", action)

    def test_source_guards_reject_fail_closed_cache_steps_and_unguarded_writers(
        self,
    ) -> None:
        source = (Path(__file__).parent / "action.yml").read_text()
        for changed in (
            source.replace("continue-on-error: true", "continue-on-error: false", 1),
            source.replace("steps.plan.outputs.save-downloads == 'true'", "true", 1),
            source.replace("steps.plan.outputs.save-build == 'true'", "true", 1),
        ):
            with self.assertRaises(ValueError):
                self.check_cache_steps(changed, 4, composite=True)
        with self.assertRaises(ValueError):
            self.check_cache_steps("steps:\n  - run: cargo test --workspace\n", 2)


if __name__ == "__main__":
    unittest.main()
