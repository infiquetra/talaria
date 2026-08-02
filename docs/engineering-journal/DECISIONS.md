# Decisions - talaria

> Repo-scoped tactical decisions with rationale and revisit conditions.

## 2026-08-01

### Name the project Talaria

**Author.** Jeff Cox / project bootstrap

**Decision.** Use `Talaria` as the project name and `infiquetra/talaria` as the public repository slug.

**Rejected alternatives.** `hermes-tui` was the clearest descriptive name, but Talaria gives the project a distinctive identity while retaining a direct Hermes reference. `mimir-tui` and `bifrost-tui` were less immediately discoverable as a Hermes TUI.

**Rationale.** The project is intended to be a serious upstream contribution candidate rather than a permanently branded private fork. Talaria is memorable and leaves room for that relationship.

**Revisit when.** Upstream Hermes adopts a conflicting name, a trademark concern appears, or the project becomes an official Hermes distribution with different naming requirements.

## 2026-08-01

### Use a fresh client with layered Hermes adapters

**Author.** Project bootstrap

**Decision.** Build a thin client around the Hermes API, optional TUI gateway, and typed Kanban adapter instead of importing Hermes core or copying the entire existing TUI.

**Rejected alternatives.** API-only loses important control-plane UX; a wholesale TUI fork carries too much existing rendering complexity and makes upstream boundaries unclear.

**Rationale.** Layered adapters preserve independent installation, make capabilities explicit, and let individual changes become focused upstream proposals.

**Revisit when.** Hermes publishes a stable external TUI SDK, the existing TUI is refactored into a smaller reusable package, or the adapter boundary proves unable to support required workflows without unacceptable duplication.
