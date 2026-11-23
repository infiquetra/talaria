# Dogfood

This page is the protocol for using Talaria to build Talaria. It says how to run a seat, what to write down when something gets in the way, and which findings may join v0.9.0. It adds no command and no setting.

## Who uses a seat

A seat is one Talaria session whose work is this repository.

On 2026-11-11 the operator's words were "probably both should dogfood". Keep both "probably" and "should". Both means the operator, by hand, and agent crews. Those words do not say that this use has already happened.

On the same day the operator's words were "maybe once these releases are defined, I can dogfood... try to use talaria to do the work for the talaria releases". Those words do not say that this use has already happened.

## How to run a seat

Use a checkout of this repository as the current directory. Start Talaria with:

    talaria

That command is the first session in the guide, [Your first session](guide.md#your-first-session). Leave the session with `/quit`.

The operator types into that session. An agent crew follows the same steps. This page does not name a second program that drives the seat.

## What to write down

When something gets in the way, file an issue in the infiquetra/talaria repository. Put the label `dogfood` on that issue. The issue includes all of these:

- what you were doing
- what happened
- what you expected
- the Talaria version
- the Hermes version

The Talaria version is the full line printed by `talaria --version`.

The Hermes version is the version on the `Hermes version` row of `talaria doctor`. That row reads `Hermes <version>; this Talaria was tested against <tag>`. Copy only `<version>`. Do not copy the status word in front of the name, the part after the semicolon, or a `Fix:` line. Do not copy the `Talaria` row. That row names the Hermes release Talaria was tested against.

Copy that text when it is `needs an answer from the gateway`, when it is `the gateway did not report a version Talaria can read`, and in every other case. The version can be the word `unknown` when the gateway sent no version string and its commit matches the tested-against record. Keep that word. Do not invent a version.

## Intake

v0.9.0 work has started at the moment the first pull request of any other v0.9.0 issue merges to `main`. Findings from dogfooding before that moment may be added to v0.9.0 freely. After that moment, at most five dogfood findings may be added to v0.9.0. The operator chooses which five. Every other finding goes to the next release. This page does not name the next release. This page does not name a board field or a milestone command.

## The scenario that proves the release

A Talaria seat takes one issue from its first prompt to a merged pull request.

This page names that scenario. It does not run the scenario. It does not say where the result is recorded.
