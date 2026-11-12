# FIX-3: Decide and record what Talaria does with a PDF attachment

Status: `proposed`. The GitHub issue for this unit is the contract; this document is its reference specification. If they disagree, the issue wins and the disagreement is reported on the issue.

How to use this document: the issue lists the questions. This file is a map of the code those questions name, with `path:line`. It does not recommend what a PDF should do.

## What Talaria does today

`sniff_image` (`crates/talaria-core/src/attachments/sniff.rs:16-28`) returns a type only for PNG, JPEG, GIF, and WebP. `unsupported_image` (`sniff.rs:33-43`) is a separate check for image bytes and extensions that are not those four. A PDF matches neither. `AttachmentEffect::ResolveDrop` (`crates/talaria-app/src/attachments.rs:1090-1121`) then builds `AttachmentEvent::DropFile`. `AttachmentEffect::AttachFile` (`attachments.rs:877-899`) calls `file.attach` after `check_file_for_attach` (`attachments.rs:1129`). That check does not look for a PDF. `pdf.attach` is the generated name at `crates/hermes-contract/src/generated/methods.rs:1290`. The normal attach test expects that name not to be called (`crates/talaria-app/tests/attachments_service.rs:900-908`).

## Where the study reads Hermes

`crates/hermes-contract/PINNED` records tag v0.22.4 on the revision this map was written against. At that tag, `file.attach` and `pdf.attach` are handlers in `tui_gateway/methods_prompt.py`. The test `test_pdf_attach_requires_poppler` is in `tests/tui_gateway/test_tui_gateway_server.py`. The study re-reads those files if the pin tag is not v0.22.4.

## What is not a decision

`docs/engineering-journal/DECISIONS.md` has no PDF entry. `docs/terminal-ui.md` does not mention a PDF. The sentence "PDF refused" is one cell of the comparison table at `docs/handoff/2026-10-22-next-steps/research/u07-hermes-ink-tui.md:340`. The journal entries now at the top of `DECISIONS.md` use a date heading, a title, and the paragraphs Context, Decision, and Costs and revisit conditions.
