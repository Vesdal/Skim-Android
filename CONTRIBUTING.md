# Contributing to Skim-Android

Hi, thanks for wanting to help!

This repo is a personal experiment, and `main` is protected: nobody can push
to it directly â€” not even the owner. Everything lands through a reviewed
pull request. That keeps malware and junk out.

## How it works

1. **Open an issue first** describing what you'd like to add or fix. Big
   changes without a prior issue will be closed â€” this is how you "ask for
   access": propose, discuss, then code.
2. **Fork** the repo, create a branch, do the work there.
3. **Open a pull request** against `main`. Small, focused PRs get reviewed
   fastest. Describe what you changed and how you tested it (device or
   emulator, Android version).
4. Wait for review. Every PR is read and tested by Vesdal before merge.

## Security rules (non-negotiable)

- No binaries, executables, or prebuilt blobs in PRs. Build outputs ship only
  through GitHub Releases made by the owner.
- No minified or obfuscated code â€” everything must be human-readable.
- No secrets, API keys, tokens, or keystores. Ever.
- No new network endpoints, permissions, or dependencies without explaining
  why in the PR. Keep new dependencies minimal and pinned.
- No changes to signing, OAuth client IDs, or update endpoints without prior
  discussion in an issue.

## License

MIT, same as upstream Skim. By contributing you agree your work lands under
the MIT license. Keep the Thunderbird OAuth attribution intact. The original
app is by [nikserg](https://github.com/nikserg/skim).
