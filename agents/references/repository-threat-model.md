# GitHub repository threat model

## Overview

The GitHub repository holds source code, build and release workflows, and maintainer automation.

The repository-specific section below may add or override trust assumptions and boundaries. When it
differs from a generic section, the repository-specific rule takes precedence for that repository.

A behavior is a security issue only when an independent attacker controls a concrete input,
repository automation uses that input to cross a boundary defined below, and the crossing grants new
power (such as repository write access), exposes publishing credentials or OpenID Connect (OIDC)
tokens, compromises source history or release artifacts, or harms downstream users. Trusted-source
compromise, intended behavior, and correctness defects that give an attacker no new power are not
security issues.

## Trust boundaries and assumptions

Maintainers, their workflow dispatch inputs, reviewed changes, protected refs, repository settings,
configured runners and environments, and explicitly trusted third-party actions are trusted. Changes
from an untrusted contributor remain untrusted when a privileged workflow runs them before review.

First-party repositories used by automation are trusted sources when their relevant branches and
workflow dispatches are restricted to trusted maintainers.

## CI and releases

Privileged workflows do not execute attacker-controlled code or promote attacker-controlled
artifacts before review or explicit authorization. Untrusted code or refs must not run with
privileged permissions or influence artifacts or other output consumed by a privileged step. The
boundary is crossed when this gives the attacker a specific credential, permission, or privileged
action that causes harm. The boundary depends on what starts each workflow, which code and artifacts
each job accepts, and which permissions, credentials, and runners those jobs receive. Unpinned
dependencies, mutable inputs, secret-shaped strings, and broad permissions do not cross it by
themselves.

## Repository-specific additions

When copying this threat model, keep the generic sections unchanged and add repository-specific
trust assumptions or security boundaries here. If a generic assumption does not apply, name it and
state the conditions for the local exception.

## Severity calibration

- **Critical:** With few prerequisites and safe defaults, a remote attacker or actor at a lower
  privilege level compromises releases or broad credentials without first compromising a declared
  trust root.
- **High:** A complete, demonstrated path from independent attacker input crosses a stated integrity
  or privilege boundary, grants material new power, and causes substantial confidentiality or
  integrity harm. It cannot depend on a trusted maintainer selecting malicious input, trust-root
  compromise, or power the attacker already has. For example, a scheduled workflow automatically
  runs mutable third-party code with repository-write, publishing, or equivalent credentials.
- **Medium:** A real but limited boundary crossing, an uncommon realistic setup, or limited
  credential or filesystem effect.
- **Low:** A narrow safety gap, limited disclosure, or a robustness problem across a real but weak
  boundary.
- **Informational:** A genuine **security** concern with no or negligible current impact.
