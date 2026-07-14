# 1. Overview

uv is a Rust CLI that resolves, builds, installs, manages, and publishes Python packages; downloads
runtimes; and updates itself. It runs with the developer's or CI worker's access to files, networks,
repositories, credentials, secrets, and OpenID Connect (OIDC) tokens.

A behavior is a security issue only when an independent attacker controls a concrete input, current
uv code or repository automation uses that input to cross a boundary defined below, and the crossing
gives the attacker new power or harms a protected asset. Trusted-source compromise, intended
behavior, and correctness defects that give an attacker no new power are not security issues.

# 2. Trust boundaries and assumptions

TLS roots, secure operator-selected mirrors, configured runners, and their intended protocol
behavior are **trust roots**; their compromise or misconfiguration alone is not a uv flaw.

Packages and their sources (indexes, Git repositories, and files) are trusted during initial
resolution or explicit lock updates. During a locked operation, the lockfile's sources, object IDs,
and hashes are authoritative; uv must not replace them in response to upstream changes.

- **Attacker-controlled:** files and metadata from an untrusted publisher; public package-name
  registrations; remote Git repositories or refs controlled by an untrusted owner; unauthenticated
  network responses; archives; malformed protocol data; and changes from an untrusted contributor
  that a privileged workflow runs before review.
- **Trusted local input for the product threat model:** the entire machine on which uv runs,
  including all environment variables; the filesystem and its links; local project files such as
  `pyproject.toml`, `uv.toml`, requirements, lockfiles, scripts, `.python-version`, and workspace
  members; installed programs and interpreters; virtual environments; PATH; network and proxy
  configuration; certificates; credentials; keyring providers; caches; and install directories. CLI
  flags, explicit requirements and scripts, maintainer-supplied workflow dispatch inputs, and
  choices such as `--allow-insecure-host`, `--no-index`, `--no-sources`, `--no-build`,
  `--only-binary`, and `--require-hashes` are also trusted. `--no-project`, `--no-config`, and
  similar isolation options require uv to ignore relevant inputs.

- uv is not normally installed setuid. Running uv as root is not considered a route to local
  privilege escalation. Selected packages may run arbitrary code. Interpreter startup, `.pth`
  loading, bytecode compilation, metadata reads, and entry points—including one named
  `python`—change timing, not authority. Execution crosses a security boundary only when it occurs
  before uv selects the relevant package, target, or interpreter; violates an effective build or
  isolation rule; crosses an actor or privilege boundary; or grants capability beyond what the
  selected package already has.
- Choosing a trusted local filesystem root authorizes normal operations on its target, including
  trusted symlinks and junctions. A path escaping its written directory or following a link is a
  security issue only if uv promised to prevent it or remote untrusted data chose the path.
  Operations that rely on `PATH` lookup or explicit relative paths are not security issues because
  `PATH`, `CWD`, and the filesystem are trusted local input. This includes placing `CWD` on `PATH`.

# 3. Threat models and security invariants

## 3.1 Product threat model: uv

The uv product runs on a trusted machine while processing package, Git, archive, and protocol data
from independent suppliers. Its security goal is to preserve the operator's choices about sources,
integrity, credentials, execution, and filesystem destinations while handling that data.

- **Sources and locks:** uv must follow its documented rules for choosing dependency sources and
  deciding whether CLI or configuration settings take precedence. Requirements files describe
  dependencies, not administrative policy, so explicit CLI options may override them. An unsupported
  option that warns and points to a supported replacement sets no policy.
- **Hashes and file identity:** A hash protects a file only when its expected value comes from a
  trusted source and covers the exact bytes uv uses, including separately downloaded compressed
  metadata. Active rules may come from requirements, constraints, lockfiles, CLI options, or
  metadata. When hash verification is active, selected bytes must match an allowed trusted hash for
  the requirement, even if the requirement does not pin a version. An existing lockfile or output
  format that requires hashes keeps that guarantee when index metadata omits them: uv must obtain
  and verify the hashes or fail. During initial resolution, missing index hashes matter only under
  an explicit required-hash policy or a format that requires them. Trusted operator runtime
  manifests may omit SHA-256 unless such a rule applies.
- **Metadata consistency:** uv may use registry-generated, embedded, cached, compressed, or
  fast-path metadata. Security-relevant fields in representations of the same package must agree; uv
  must reject mismatches before selecting, locking, building, or installing it.
- **Caches:** Same-user caches and installed programs belong to the trusted machine. Cached data
  crosses a security boundary when data accepted for one independently controlled package or source
  is reused for another without that request's required checks. Mutable-source reuse breaks no
  freshness or revocation promise unless one is documented. Trusted cached metadata may establish
  identity before uv decides whether to run a build backend.
- **Network and credentials:** Credentials stay within their documented origin, path, realm, and
  audience. A redirect crosses this boundary only when an independent attacker controls it and an
  unauthorized recipient can use the credential. Registry compromise or misconfiguration alone does
  not cross the boundary. uv must not expose credentials through URLs, errors, subprocess arguments,
  cache keys, or displayed output.
- **Builds and metadata:** Selected source distributions and Git dependencies may run build
  backends. Dynamic identity may delay package-specific rules. For pip compatibility, `--no-build`
  means binary-only selection but permits backend execution for an explicitly selected editable
  source unless a stronger policy applies. Build-backend execution crosses a security boundary only
  if it happens before package selection, bypasses the rules that actually apply, escapes build
  isolation, or runs with greater privilege.
- **Archives, installation, and cleanup:** Selecting an artifact authorizes its files and build
  code. Its metadata must not cause writes or deletions outside the chosen root and any
  intentionally followed symlink or junction targets. Trusted `.venv`, cache, configuration,
  symlink, or junction state does not create an escape by itself. Platform-specific rejection and
  containment rules define the boundary.
- **Generated shell code:** Generated shell code quotes untrusted values for its target shell.
  Output that requires later execution or sourcing has a limited, multi-step impact. Automatic or
  privileged use, or a comparable increase in authority, raises the impact.
- **Git identity:** When a documented 40-character hexadecimal value names a commit, uv must resolve
  it as that immutable Git object; a branch or tag with the same name must not override it. For a
  hexadecimal identifier shorter than 40 characters, an attacker-controlled ref crosses the Git
  identity boundary if it redirects the identifier from its intended Git object. The identifier's
  length and whether the lockfile records the full object ID determine whether that redirect is
  possible. Undocumented legacy query or fragment pins should be rejected or migrated rather than
  silently unpinned. Non-hex branches and tags are intentionally mutable. uv must not silently
  replace an immutable Git reference with a mutable one, send credentials outside their approved
  destination, or violate a documented integrity guarantee. Operator-selected transports and
  endpoints are trusted.
- **Runtimes and updates:** Built-in metadata and explicit integrity promises bind the selected
  bytes. Configured HTTPS vendors and secure mirrors are trust roots, so the absence of another
  checksum embedded in uv does not establish attacker control. For Ruff metadata,
  `astral-sh/versions`, its maintainers and GitHub controls, authenticated `main`, and HTTPS are
  trusted. That manifest's checksum is a valid trust root; mutability alone does not require another
  uv-embedded digest.
- **Stored credentials, publishing, and OIDC:** The impact of exposing a stored credential depends
  on its issuer, audience, intended and unauthorized recipients, validity, usable authority, and
  reach. Material impact includes private-data access, writes, publication, trusted execution,
  lateral movement, or privilege escalation. A credential has negligible impact only if it is
  single-purpose and its effective authority is read-only and limited to inert test infrastructure.
- **Availability:** Local input and authenticated metadata accepted during initial resolution or a
  lock update are trusted for that operation. Resource exhaustion from them is a performance bug,
  not a security issue. Under an existing lockfile, changed remote state has no authority;
  processing it is a security issue if it reliably causes excessive recursion, stack growth, or
  decompression, or exhausts memory, CPU, disk, or network resources. Unauthenticated remote
  protocol data remains attacker-controlled. A one-shot parser panic is a correctness bug, not a
  security issue.

## 3.2 Repository threat model: GitHub

The GitHub repository holds uv's source, build and release workflows, and maintainer automation.
Maintainers, reviewed changes, protected refs, repository settings, configured runners and
environments, and explicitly trusted third-party actions are trusted. Untrusted inputs include
public contributions, workflow inputs supplied by untrusted actors, third-party refs or actions
outside that trust set, and artifacts passed from untrusted jobs. The protected assets are source
history, release artifacts, publishing credentials and OIDC tokens, repository writes, and
downstream users.

- **CI and releases:** Privileged workflows do not execute attacker-controlled code or promote
  attacker-controlled artifacts before review or explicit authorization. Untrusted code or refs must
  not run with privileged permissions or influence artifacts or other output consumed by a
  privileged step. The boundary is crossed when this gives the attacker a specific credential,
  permission, or privileged action that causes harm. The boundary depends on what starts each
  workflow, which code and artifacts each job accepts, and which permissions, credentials, and
  runners those jobs receive. Unpinned dependencies, mutable inputs, secret-shaped strings, and
  broad permissions do not cross it by themselves.

# 4. Severity calibration

- **Critical:** With few prerequisites and safe defaults, a remote attacker or actor at a lower
  privilege level compromises updates, runtimes, releases, broad credentials, or arbitrary files
  without first compromising a declared trust root.
- **High:** A complete, demonstrated path from independent attacker input crosses a stated integrity
  or privilege boundary, grants material new power, and causes substantial confidentiality or
  integrity harm. It cannot depend on a trusted maintainer selecting malicious input, trust-root
  compromise, or power the attacker already has. Examples: execute attacker bytes instead of a
  documented full 40-hex immutable Git pin, or automatically run mutable third-party code in a
  scheduled workflow with repository-write, publishing, or equivalent credentials.
- **Medium:** A real but limited boundary crossing, an uncommon realistic setup, limited credential
  or filesystem effect, reliable resource exhaustion from remote data that has no authority under a
  locked operation, or premature execution that violates an effective explicit policy. Examples:
  loss of a trusted required hash in an explicitly run tool environment; collision between an
  abbreviated or undocumented legacy Git pin and a mutable ref; or shell injection that waits for
  explicit activation-script sourcing. These become High only when a separate privileged consumer,
  credential, or authoritative output materially increases impact.
- **Low:** A narrow safety gap, rare reuse of one cache entry across independently authorized
  identities, limited disclosure, or a robustness problem across a real but weak boundary.
- **Informational:** A genuine **security** concern with no or negligible current impact.
