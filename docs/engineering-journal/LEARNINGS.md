# Learnings - talaria

> Empirical findings, mechanisms, fixes, validations, and generalizable rules. Keep newest entries first.

## 2026-08-01

### Hermes already exposes two useful client boundaries

**Author.** Project bootstrap

**Context.** The project needed to decide whether a new TUI should call only an OpenAI-compatible endpoint, import Hermes internals, or use the existing Hermes TUI architecture.

**Evidence.** Public Hermes source areas include `ui-tui/`, `tui_gateway/`, and `gateway/platforms/api_server.py`.

**Mechanism.** The API server is the better portable run/session boundary, while the TUI gateway carries richer Hermes-native control-plane behavior. They solve different integration problems.

**Fix.** Talaria uses an API-first direction with an optional gateway adapter and typed adapters for non-chat surfaces.

**Validation.** The direction is documented in [the project analysis](../analysis/2026-08-01-hermes-tui-project-direction.md), with the initial prototype checks passing once dependencies are installed.

**Generalizable rule.** Do not mistake a generic chat protocol for a complete agent control-plane contract.
