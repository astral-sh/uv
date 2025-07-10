# Security policy

## Scope of security vulnerabilities

uv is a Python package manager. Due to the design of the Python packaging ecosystem and the dynamic
nature of Python itself, there are many cases where uv can execute arbitrary code. For example:

- uv invokes Python interpreters on the system to retrieve metadata
- uv builds source distributions as described by PEP 517
- uv may build packages from the requested package indexes

These are not considered vulnerabilities in uv. If you think uv's stance in these areas can be
hardened, please file an issue for a new feature.

## Reporting a vulnerability

If you have found a possible vulnerability that is not excluded by the above
[scope](#scope-of-security-vulnerabilities), please email `security at astral dot sh`.

## Bug bounties

While we sincerely appreciate and encourage reports of suspected security problems, please note that
Astral does not currently run any bug bounty programs.

## Vulnerability disclosures

Critical vulnerabilities will be disclosed via GitHub's
[security advisory](https://github.com/astral-sh/uv/security) system.
