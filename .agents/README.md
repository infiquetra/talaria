# Talaria development skills

Codex reads these skills from `.agents/skills/`. Claude Code uses the same files through links in `.claude/skills/`.

| Skill | Use it for | Installed source |
| --- | --- | --- |
| `tui-design` | Mock up alternative layouts, render terminal cells, and compare designs at different sizes and color depths. | [diegomarino/tui-design at bc81639](https://github.com/diegomarino/tui-design/tree/bc81639c83fb01feb74429454e4ad50706b7bb86/skills/tui-design) |
| `tui-usability-review` | Review keyboard input, focus, resizing, and selection as live data changes. Talaria gives this skill its own name and description because upstream also calls it `tui-design`. | [hyperb1iss/hyperskills v3.13.4](https://github.com/hyperb1iss/hyperskills/tree/d9d7f46806ea7c0a8b6c45b92a3904542700dae3/skills/tui-design) |
| `tui-test` | Drive a terminal, send keys, resize its window, and assert what the user sees. | [microsoft/tui-test 0.1.1](https://github.com/microsoft/tui-test/tree/76b0ab553c7c56dd30117414acb27cc5972a4707) |
| `rust-performance` | Measure a slow interaction, find where it spends time, and compare the result after a change. | [po4yka/rust-skills v0.5.0](https://github.com/po4yka/rust-skills/tree/0d0b9462c8f51532253af6e20458d934b24c8bf7/skills/rust-performance) |

For Talaria designs, keep finished output in native scrollback and use the project's terminal setup. Start performance investigations with the existing replay benchmarks and latency budgets. [Terminal patterns](../docs/agents/patterns.md) · [Benchmark guide](../tools/bench/README.md)

## Terminal test runner

Use `.agents/bin/tui-test` from the repository root. Each checkout needs the executable for its own platform. To install the pinned release on macOS or Linux, run the following from the repository root.

```sh
curl -fsSL https://raw.githubusercontent.com/microsoft/tui-test/76b0ab553c7c56dd30117414acb27cc5972a4707/install/install.sh \
  | TUI_TEST_VERSION=0.1.1 TUI_TEST_INSTALL_DIR="$PWD/.agents/bin" TUI_TEST_NO_MODIFY_PATH=1 sh
.agents/bin/tui-test --version
```

On Windows, extract `tui-test.exe` from the Windows release archive into `.agents/bin/`. [Platform downloads](https://github.com/microsoft/tui-test/releases/tag/0.1.1)
