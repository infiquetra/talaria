# Live smoke test

Drives the built binary in a Herdr pane against a running gateway and checks five behaviours.

## Usage

Run `cargo run -q -p smoke -- run` from a Herdr pane.

## Preconditions

A running gateway, the fixture profile and a built binary.

## Exit codes

0 passed, 1 failed, 2 preflight failed, 3 passed with a skip.

## Steps

Startup, plain answer, approval, interrupt, quit and resume.

## Reading a failure

A pane text that reports the gateway or profile unavailable makes the acceptance result blocked by the environment, not failed.
