"""Offline cache routing and trust-boundary checks; no Cargo work or cache service."""

import re
import tempfile
import unittest
from pathlib import Path

import cache

RELEASE_CACHE_IF = (
    "github.ref == format('refs/tags/{0}', steps.resolve.outputs.tag) && "
    "hashFiles('.github/actions/rust-cache/action.yml') != ''"
)


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

    def test_release_saves_stay_within_trusted_tag(self) -> None:
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
        for event in ("push", "schedule", "workflow_dispatch"):
            for ref in ("refs/heads/main", "refs/heads/topic"):
                result = self.plan(
                    "release-debug", GITHUB_EVENT_NAME=event, GITHUB_REF=ref
                )
                self.assertEqual(result["save-downloads"], "false")
                self.assertEqual(result["save-build"], "false")
        dispatched = self.plan(
            "release-debug",
            GITHUB_EVENT_NAME="workflow_dispatch",
            GITHUB_REF="refs/tags/v1.2.3",
        )
        self.assertEqual(dispatched["save-downloads"], "false")
        self.assertEqual(dispatched["save-build"], "false")


class WorkflowPolicyTests(unittest.TestCase):
    @staticmethod
    def field(block: str, key: str, indent: int) -> str:
        values = re.findall(
            r"(?m)^" + " " * indent + re.escape(key) + r": ([^\n]+)$", block
        )
        if len(values) != 1:
            raise ValueError(f"missing or duplicate cache field: {key}")
        return values[0]

    @classmethod
    def check_job_bindings(
        cls, source: str, expected: dict[str, tuple[str, str]]
    ) -> None:
        # Check the existing, deliberately small workflow source shape without a YAML dependency.
        parts = re.split(r"(?m)^  ([\w-]+):[ \t]*$", source.split("\njobs:\n", 1)[1])
        jobs = dict(zip(parts[1::2], parts[2::2]))
        if len(jobs) != len(parts[1::2]) or jobs.keys() != expected.keys():
            raise ValueError("workflow jobs differ from the cache policy")
        for job, (role, path) in expected.items():
            blocks = re.split(r"(?m)^      - ", jobs[job])[1:]
            caches = [
                block
                for block in blocks
                if re.search(
                    r"(?m)^        uses: \./\.github/actions/rust-cache$", block
                )
            ]
            if len(caches) != 1:
                raise ValueError(f"missing or duplicate cache for {job}")
            actual_role = cls.field(caches[0], "role", 10)
            if actual_role != role or cache.BUILD_PATHS.get(actual_role) != path:
                raise ValueError(f"wrong cache role or path for {job}")
            if job == "release" and cls.field(caches[0], "if", 8) != RELEASE_CACHE_IF:
                raise ValueError(
                    "release cache must bind the event ref to the checked-out tag"
                )

    def test_source_guard_rejects_broadened_writer_authority(self) -> None:
        source = (Path(__file__).parent / "action.yml").read_text()
        for authority in ("save-downloads", "save-build"):
            condition = f"steps.plan.outputs.{authority} == 'true'"
            with self.subTest(authority=authority), self.assertRaises(ValueError):
                self.check_cache_steps(
                    source.replace(condition, condition + " || true", 1),
                    4,
                    composite=True,
                )

    def test_release_cache_requires_bound_tag_ref_and_existing_helper(self) -> None:
        root = Path(__file__).resolve().parents[3]
        source = (root / ".github/workflows/release.yml").read_text()
        expected = {"release": ("release-debug", "target/debug")}
        for weakened in (
            "hashFiles('.github/actions/rust-cache/action.yml') != ''",
            "github.ref == format('refs/tags/{0}', steps.resolve.outputs.tag)",
            RELEASE_CACHE_IF + " || true",
        ):
            with self.subTest(condition=weakened), self.assertRaises(ValueError):
                self.check_job_bindings(
                    source.replace(RELEASE_CACHE_IF, weakened), expected
                )

    def test_each_job_role_and_path_binding_is_checked(self) -> None:
        root = Path(__file__).resolve().parents[3]
        source = (root / ".github/workflows/ci.yml").read_text()
        swapped = (
            source.replace("role: native-debug", "role: temporary")
            .replace("role: coverage-debug", "role: native-debug")
            .replace("role: temporary", "role: coverage-debug")
        )
        expected = {
            "rust-check": ("native-debug", "target/debug"),
            "rust-coverage": ("coverage-debug", "target/llvm-cov-target/debug"),
        }
        with self.assertRaises(ValueError):
            self.check_job_bindings(swapped, expected)
        original = cache.BUILD_PATHS["coverage-debug"]
        try:
            cache.BUILD_PATHS["coverage-debug"] = "target/debug"
            with self.assertRaises(ValueError):
                self.check_job_bindings(source, expected)
        finally:
            cache.BUILD_PATHS["coverage-debug"] = original

    @classmethod
    def check_cache_steps(
        cls, source: str, indent: int, composite: bool = False
    ) -> None:
        blocks = re.split(r"(?m)^" + " " * indent + r"- ", source)[1:]
        uses = 0
        for block in blocks:
            if (
                "uses: actions/cache" not in block
                and "uses: ./.github/actions/rust-cache" not in block
            ):
                continue
            uses += 1
            if cls.field(block, "continue-on-error", indent + 2) != "true":
                raise ValueError("cache failure could block original work")
            if composite:
                key = cls.field(block, "key", indent + 4)
                if key == "${{ steps.plan.outputs.download-key }}":
                    authority, prefix = "save-downloads", ""
                elif key == "${{ steps.plan.outputs.build-key }}":
                    authority, prefix = (
                        "save-build",
                        "steps.plan.outputs.build-path != '' && ",
                    )
                    if (
                        cls.field(block, "path", indent + 4)
                        != "${{ steps.plan.outputs.build-path }}"
                    ):
                        raise ValueError(
                            "build cache does not use the planned role path"
                        )
                else:
                    raise ValueError("unknown cache key")
                action = cls.field(block, "uses", indent + 2)
                if (
                    action
                    == "actions/cache/restore@0057852bfaa89a56745cba8c7296529d2fc39830 # v4.3.0"
                ):
                    operator = "!="
                elif (
                    action
                    == "actions/cache@0057852bfaa89a56745cba8c7296529d2fc39830 # v4.3.0"
                ):
                    operator = "=="
                else:
                    raise ValueError("unexpected cache action")
                expected = prefix + f"steps.plan.outputs.{authority} {operator} 'true'"
                if cls.field(block, "if", indent + 2) != expected:
                    raise ValueError(
                        "cache writer or reader has lost its authority guard"
                    )
        if not uses:
            raise ValueError("missing cache integration")

    def test_all_roles_use_optional_caches_without_gating_original_work(self) -> None:
        root = Path(__file__).resolve().parents[3]
        roles = {
            "ci.yml": {
                "rust-check": ("native-debug", "target/debug"),
                "rust-coverage": ("coverage-debug", "target/llvm-cov-target/debug"),
            },
            "bench.yml": {"bench": ("bench-release", "target/release")},
            "contract.yml": {
                "contract-gen-check": ("contract-check-debug", "target/debug"),
                "contract-drift": ("contract-drift", ""),
            },
            "release.yml": {"release": ("release-debug", "target/debug")},
        }
        for name, expected in roles.items():
            source = (root / ".github/workflows" / name).read_text()
            with self.subTest(workflow=name):
                self.check_cache_steps(source, 6)
                self.check_job_bindings(source, expected)
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
