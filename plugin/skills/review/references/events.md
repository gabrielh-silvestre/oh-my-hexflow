# Check events

A check runs at one of 4 events:

- `before-commit`: the git `pre-commit` and `pre-merge-commit` hooks.
- `before-push`: the git `pre-push` hook; `gate-delivery` and `close` require its attestation.
- `before-merge`: run by hand with `omhl check run --event before-merge`; there is no hook for it.
- `before-publishing-review-comments`: an instruction to the agent before it publishes review comments; there is no attestation.

A project binds a check to any of the 4 events. An invented event is refused.
