Before reporting a security issue, please understand what this project is.

# Security policy

## Scope

UnsoftOne is a **local-first, offline desktop application**. There are no
servers, no accounts, and no network services operated by this project.

Please see
[ADR-0007](https://github.com/temidayoxyz/unsoftone/blob/main/docs/adr/0007-local-first-is-not-negotiable.md)
— local-first operation is an architectural constraint, not a preference.

## What counts as a vulnerability

Realistically, in scope:

- **Memory safety or parsing bugs reachable from opening a malicious file.**
  This project parses untrusted, malformed documents by design; a crash or memory
  corruption bug in that path is a genuine security issue.
- **Data loss or corruption** reachable through normal use, especially via a
  round-trip failure.
- **A privacy violation** — any collection of telemetry, any network call, any
  local data leaving the machine. These are treated as severe because they
  contradict the project's core promise.
- **Code execution** reachable from a document without user action.

## What is not a vulnerability

- Malformed files that cause a clean error message. Failing safely is correct
  behaviour.
- Missing features, formatting differences, compatibility gaps. Those belong in
  the normal issue tracker.
- Anything requiring the user to have already run untrusted code.

## Reporting

Report privately via
[GitHub Security Advisories](https://github.com/temidayoxyz/unsoftone/security/advisories/new)
on the `unsoftone` repository, or contact the maintainer directly.

Please do not open a public issue for a security bug.

Include the sample file if you have one — reproduction files are extremely
valuable for this class of bug. **Redact anything you are not permitted to
share.**

We aim to acknowledge reports within a few days. There is no bounty programme,
and there is no formal support commitment: this is an early-stage volunteer
project. Please factor that in when deciding what to report.
