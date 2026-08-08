---
title: Claiming the `talaria` name on the Python Package Index
type: chore
status: active
date: 2026-08-08
origin: docs/plans/2026-08-07-v0-1-release-and-install-plan.md
---

# Claiming the `talaria` name on the Python Package Index

This is step S5 of the release plan. It is off the critical path: v0.1.0 ships from a git tag and its
outcome changes nothing about S1–S4.

## What is actually true, verified 2026-08-08

**The name is taken by an unrelated project.** `talaria` on the Python Package Index is "Talaria, a
geek-friendly content management system", by Roman Imankulov, GNU GPLv2+. One release has ever been
made: version 0.2.0, uploaded **2010-06-19**, a 12,758-byte source tarball. Nothing since.

**The package declares its own abandonment.** Its classifier list contains
`Development Status :: 7 - Inactive`. That is the author's own statement, in the package metadata,
and it is a stronger fact than anything an outside observer could assert.

**No source repository for it survives.** A search of public repositories for the project turns up
nothing, and the package's description links none.

**But the author is active and reachable, and this is the finding that changes the plan.** The
project homepage recorded in the package metadata (`imankulov.name`) still resolves — it redirects to
a live personal site. The matching GitHub account has 128 public repositories and profile activity as
recent as **2026-06-05**, with a public contact address. This is not a lapsed account.

## What follows from that

The plan assumed an abandoned project with an unreachable owner, which is the case PEP 541's
name-claim process exists for. That assumption is wrong. PEP 541 requests are for disputes that
cannot be settled directly, the moderation queue is long, and the issue template's
**"Contact and additional research"** field is required precisely because a request filed without a
contact attempt is a request that skipped the obvious step.

**So: ask him first.** An active open-source developer, asked politely about a name he marked
Inactive sixteen years ago on a project with one release and no surviving repository, is quite
likely to simply say yes — and that path is faster than moderation, does not consume anybody's
volunteer time, and is the courteous order of operations. If he declines, that is his call and the
name is his; Talaria installs from a git tag either way.

File the PEP 541 request only if there is **no reply after a reasonable wait** — six weeks is the
conventional window. At that point the contact attempt is documented, which is exactly what the form
asks for.

**This is not a deferral of S5.** It is S5 done in the order the evidence supports. Both artifacts
below are written and ready; what they need is a human to send one and, later, possibly the other.

## Blocked on the operator, and why

Two things cannot be supplied from here:

1. **The contact attempt itself.** Sending mail to a third party as Jeff Cox is not something to do
   on his behalf without him seeing the words first. The draft is below; the addresses are in the
   [PyPI page's metadata](https://pypi.org/project/Talaria/) and on the
   [GitHub profile](https://github.com/imankulov) and are deliberately not copied into this public
   repository.
2. **A PyPI username.** The issue template requires one and validates it as a link to
   `https://pypi.org/user/<name>`. There is no way to determine it from this repository, and
   guessing it would put a wrong name in front of a moderator.

## Draft: the message to send

> Subject: The `talaria` name on PyPI
>
> Hello Roman,
>
> I maintain a small open-source project called Talaria — a terminal user interface for the Hermes
> agent runtime — at https://github.com/infiquetra/talaria. It is MIT-licensed and currently
> installs from a git tag, because the `talaria` name on the Python Package Index is taken by your
> content management system from 2010.
>
> I noticed that project is marked `Development Status :: 7 - Inactive` and has had one release. If
> you have no further plans for the name, would you be willing to transfer it? PyPI has a process
> for this and I am happy to do the paperwork from my side, or to drop the idea entirely if you would
> rather keep it — either answer is completely fine, and I would rather ask than go around you.
>
> Thanks for your time,
> Jeff Cox

## Draft: the PEP 541 request, if there is no reply

File at <https://github.com/pypi/support/issues/new?template=pep541-request.yml>. Title:
`PEP 541 Request: talaria`.

**Project to be claimed**

```
`talaria`: https://pypi.org/project/talaria
```

**Your PyPI username** — _operator to supply_

```
`<USERNAME>`: https://pypi.org/user/<USERNAME>
```

**Reasons for the request**

> The project has been abandoned by any reasonable reading, and says so itself.
>
> - It has exactly one release, version 0.2.0, uploaded on 2010-06-19 — sixteen years ago.
> - Its own metadata carries the classifier `Development Status :: 7 - Inactive`.
> - No public source repository for it survives; the package links none and searching finds none.
> - It is a content management system, with no relationship to the project requesting the name.
>
> I contacted the current owner directly on <DATE> at the address in the package metadata and have
> had no reply in the <N> weeks since. I would still prefer his consent to this process, and if he
> responds at any point I will withdraw this request.

**Maintenance or replacement?** — `Replacement`

**Source code repositories URLs**

> - Current project: none known. The package predates its author's public repository history and
>   links no repository; a search for it returns nothing.
> - Proposed project: https://github.com/infiquetra/talaria — MIT, actively developed, released
>   v0.1.0 on 2026-08-08.

**Contact and additional research** — _operator to complete the date and outcome_

> - Reviewed the package's PyPI metadata on 2026-08-08: sole release 0.2.0 dated 2010-06-19,
>   12,758 bytes, GNU GPLv2+, classifier `Development Status :: 7 - Inactive`.
> - The homepage in the metadata (`imankulov.name`) redirects to the author's current personal site,
>   so the author is active even though this package is not. He is therefore reachable, and I
>   treated contacting him as the first step rather than filing here immediately.
> - Emailed the author on <DATE> at the address published in the package metadata, asking whether he
>   would be willing to transfer the name and making clear that a refusal was fine. <OUTCOME>
> - Searched public repositories for a surviving source repository for the 2010 project: none found.

## What this deliberately does not do

- **It does not publish Talaria to PyPI even if the name is granted.** That is a separate decision,
  and the v0.1 verdict's limits — nobody has driven the interface on Linux, no run has used a real
  terminal emulator — are the reason it has not been taken. Holding a name is not the same as
  claiming a package index's implicit "this is for anyone, on any machine".
- **It does not rename Talaria to `talaria-tui` as a workaround.** That would permanently split the
  install name from the import name in exchange for reach this release is not seeking.
