# Changelog

## 0.2.36

### Preview features

- Add resolver error context to `run` and `tool run` ([#5991](https://github.com/astral-sh/uv/pull/5991))
- Avoid replacing executables on no-op upgrades ([#5998](https://github.com/astral-sh/uv/pull/5998))
- Colocate Python install cache with destination directory ([#6043](https://github.com/astral-sh/uv/pull/6043))
- Filter mixed sources from `--find-links` entries in lockfile ([#6025](https://github.com/astral-sh/uv/pull/6025))
- Fix some outdated documentation discussing Python environments ([#6058](https://github.com/astral-sh/uv/pull/6058))
- Fix projects guide typo ([#6033](https://github.com/astral-sh/uv/pull/6033))
- Fix tools guide typo ([#6027](https://github.com/astral-sh/uv/pull/6027))
- Hide python options in `uv tool list` help ([#6003](https://github.com/astral-sh/uv/pull/6003))
- Improve top-level help for `uv tool` commands ([#5983](https://github.com/astral-sh/uv/pull/5983))
- Move help documentation into dedicated page ([#6057](https://github.com/astral-sh/uv/pull/6057))
- Remove `editable: false` support ([#5987](https://github.com/astral-sh/uv/pull/5987))
- Remove uses of `Option<MarkerTree>` in `ResolutionGraph` ([#6035](https://github.com/astral-sh/uv/pull/6035))
- Resolve relative `tool.uv.sources` relative to containing project ([#6045](https://github.com/astral-sh/uv/pull/6045))
- Support PEP 723 scripts in `uv add` and `uv remove` ([#5995](https://github.com/astral-sh/uv/pull/5995))
- Support `tool.uv` in PEP 723 scripts ([#5990](https://github.com/astral-sh/uv/pull/5990))
- Treat local indexes as registry sources in lockfile ([#6016](https://github.com/astral-sh/uv/pull/6016))
- Use simplified paths in lockfile ([#6049](https://github.com/astral-sh/uv/pull/6049))
- Use upgrade-specific output for tool upgrade ([#5997](https://github.com/astral-sh/uv/pull/5997))

## 0.2.35

### Preview features

- Add CLI flags to reference documentation ([#5926](https://github.com/astral-sh/uv/pull/5926))
- Add `update` alias for `uv tool upgrade` ([#5948](https://github.com/astral-sh/uv/pull/5948))
- Add caveat about pip interface name ([#5940](https://github.com/astral-sh/uv/pull/5940))
- Add hint for long help to `uvx` ([#5971](https://github.com/astral-sh/uv/pull/5971))
- Avoid requires-python warning in virtual-only workspace ([#5895](https://github.com/astral-sh/uv/pull/5895))
- Discard forks when using `--upgrade` ([#5905](https://github.com/astral-sh/uv/pull/5905))
- Document the `tool upgrade` command ([#5947](https://github.com/astral-sh/uv/pull/5947))
- Document virtual environment discovery ([#5965](https://github.com/astral-sh/uv/pull/5965))
- Enable mirror for `python-build-standalone` downloads ([#5719](https://github.com/astral-sh/uv/pull/5719))
- Fix reuse of Git commits in lockfile ([#5908](https://github.com/astral-sh/uv/pull/5908))
- Ignore local configuration in tool commands ([#5923](https://github.com/astral-sh/uv/pull/5923))
- Improve the CLI documentation for `uv add` ([#5914](https://github.com/astral-sh/uv/pull/5914))
- Improve the CLI documentation for `uv remove` ([#5916](https://github.com/astral-sh/uv/pull/5916))
- Improve the `uv lock` CLI documentation ([#5932](https://github.com/astral-sh/uv/pull/5932))
- Improve the `uv python` CLI documentation ([#5961](https://github.com/astral-sh/uv/pull/5961))
- Improve the `uv sync` CLI documentation ([#5930](https://github.com/astral-sh/uv/pull/5930))
- Improve the `uv tree` CLI documentation ([#5917](https://github.com/astral-sh/uv/pull/5917))
- Fix link to tools concept page ([#5906](https://github.com/astral-sh/uv/pull/5906))
- Add `uv tool upgrade` command ([#5197](https://github.com/astral-sh/uv/pull/5197))
- Implement marker trees using algebraic decision diagrams ([#5898](https://github.com/astral-sh/uv/pull/5898))
- Make repeated `uv add` operations simpler ([#5922](https://github.com/astral-sh/uv/pull/5922))
- Move some documents to relevant sections ([#5968](https://github.com/astral-sh/uv/pull/5968))
- Rename `distribution` to `packages` in lockfile ([#5861](https://github.com/astral-sh/uv/pull/5861))
- Respect `--upgrade-package` in tool install ([#5941](https://github.com/astral-sh/uv/pull/5941))
- Respect `--upgrade-package` when resolving from lockfile ([#5907](https://github.com/astral-sh/uv/pull/5907))
- Retain and respect settings in tool upgrades ([#5937](https://github.com/astral-sh/uv/pull/5937))
- Search beyond workspace root when discovering configuration ([#5931](https://github.com/astral-sh/uv/pull/5931))
- Show build and install summaries in `uv run` and `uv tool run` ([#5899](https://github.com/astral-sh/uv/pull/5899))
- Support relative path wheels ([#5969](https://github.com/astral-sh/uv/pull/5969))
- Update the interface for declaring Python download preferences ([#5936](https://github.com/astral-sh/uv/pull/5936))
- Use cached environments for `--with` layers ([#5897](https://github.com/astral-sh/uv/pull/5897))
- Warn when project-specific settings are passed to non-project `uv run` commands ([#5977](https://github.com/astral-sh/uv/pull/5977))

## 0.2.34

### Preview features

- Add "next steps" to some early documentation pages ([#5825](https://github.com/astral-sh/uv/pull/5825))
- Add `--no-build-isolation` to uv lock et al ([#5829](https://github.com/astral-sh/uv/pull/5829))
- Add `--no-sources` to avoid reading from `tool.uv.sources` ([#5801](https://github.com/astral-sh/uv/pull/5801))
- Add `uv add --no-sync` and `uv remove --no-sync` ([#5881](https://github.com/astral-sh/uv/pull/5881))
- Add a guide for publishing packages ([#5794](https://github.com/astral-sh/uv/pull/5794))
- Address some feedback in the tools documentation ([#5827](https://github.com/astral-sh/uv/pull/5827))
- Avoid lingering dev and optional dependencies in `uv tree` ([#5766](https://github.com/astral-sh/uv/pull/5766))
- Avoid mismatch in `--locked` with Git dependencies ([#5865](https://github.com/astral-sh/uv/pull/5865))
- Avoid panic when re-locking with precise commit ([#5863](https://github.com/astral-sh/uv/pull/5863))
- Avoid using already-installed tools on `--upgrade` or `--reinstall` ([#5799](https://github.com/astral-sh/uv/pull/5799))
- Better workspace documentation ([#5728](https://github.com/astral-sh/uv/pull/5728))
- Collapse policies section into reference ([#5696](https://github.com/astral-sh/uv/pull/5696))
- Don't show deprecated warning in `uvx --isolated` ([#5798](https://github.com/astral-sh/uv/pull/5798))
- Ensure `python`-to-`pythonX.Y` symlink exists in downloaded Pythons ([#5849](https://github.com/astral-sh/uv/pull/5849))
- Fix CLI reference URLs to subcommands ([#5722](https://github.com/astral-sh/uv/pull/5722))
- Fix some console blocks in the environment doc ([#5826](https://github.com/astral-sh/uv/pull/5826))
- Group resolver options in lockfile ([#5853](https://github.com/astral-sh/uv/pull/5853))
- Improve CLI documentation for `uv tree` ([#5870](https://github.com/astral-sh/uv/pull/5870))
- Improve documentation for `uv init` CLI ([#5862](https://github.com/astral-sh/uv/pull/5862))
- Improvements to the documentation ([#5718](https://github.com/astral-sh/uv/pull/5718))
- Link to the GitHub integration guide from the cache concept ([#5828](https://github.com/astral-sh/uv/pull/5828))
- Make some minor tweaks to the docs ([#5786](https://github.com/astral-sh/uv/pull/5786))
- Omit local segments when adding uv add bounds ([#5753](https://github.com/astral-sh/uv/pull/5753))
- Remove top-level bar from Python installs ([#5788](https://github.com/astral-sh/uv/pull/5788))
- Replace `uv help python` references in CLI documentation with links ([#5871](https://github.com/astral-sh/uv/pull/5871))
- Respect `.python-version` in `--isolated` runs ([#5741](https://github.com/astral-sh/uv/pull/5741))
- Respect malformed `.dist-info` directories in tool installs ([#5756](https://github.com/astral-sh/uv/pull/5756))
- Reuse existing virtualenvs with `--no-project` ([#5846](https://github.com/astral-sh/uv/pull/5846))
- Rewrite resolver docs ([#5723](https://github.com/astral-sh/uv/pull/5723))
- Show default and possible options in CLI reference documentation ([#5720](https://github.com/astral-sh/uv/pull/5720))
- Skip files when detecting workspace members ([#5735](https://github.com/astral-sh/uv/pull/5735))
- Support empty dependencies in PEP 723 scripts ([#5864](https://github.com/astral-sh/uv/pull/5864))
- Support uv add `--dev` in virtual workspaces ([#5821](https://github.com/astral-sh/uv/pull/5821))
- Update documentation index ([#5824](https://github.com/astral-sh/uv/pull/5824))
- Update resolver reference documentation ([#5823](https://github.com/astral-sh/uv/pull/5823))
- Update the override section with some content from the README ([#5820](https://github.com/astral-sh/uv/pull/5820))
- Update the resolution concept documentation ([#5813](https://github.com/astral-sh/uv/pull/5813))
- Use cache for Python install temporary directories ([#5787](https://github.com/astral-sh/uv/pull/5787))
- Use lockfile directly in `uv tree` ([#5761](https://github.com/astral-sh/uv/pull/5761))
- Use uv installer during build ([#5854](https://github.com/astral-sh/uv/pull/5854))
- Filter `uv tree` to current platform by default ([#5763](https://github.com/astral-sh/uv/pull/5763))
- Redact registry credentials in lockfile ([#5803](https://github.com/astral-sh/uv/pull/5803))
- Show extras and dev dependencies in `uv tree` ([#5768](https://github.com/astral-sh/uv/pull/5768))
- Support `--python-platform` in `uv tree` ([#5764](https://github.com/astral-sh/uv/pull/5764))
- Add help heading for `--no-sources` ([#5833](https://github.com/astral-sh/uv/pull/5833))
- Avoid reusing incompatible distributions across lock and sync ([#5845](https://github.com/astral-sh/uv/pull/5845))
- Fix broken anchor links in docs about dependencies ([#5769](https://github.com/astral-sh/uv/pull/5769))
- Fix the default value of python-preference in docs/reference/settings.md ([#5755](https://github.com/astral-sh/uv/pull/5755))
- Improve CLI documentation for `uv run` ([#5841](https://github.com/astral-sh/uv/pull/5841))
- Remove some trailing backticks from the docs ([#5781](https://github.com/astral-sh/uv/pull/5781))
- Use `uvx` in docs serve contributing command ([#5795](https://github.com/astral-sh/uv/pull/5795))

## 0.2.33

### Preview features

- Add `--package` to `uv sync` ([#5656](https://github.com/astral-sh/uv/pull/5656))
- Add documentation for caching the uv cache in GHA ([#5663](https://github.com/astral-sh/uv/pull/5663))
- Avoid persisting `uv add` calls that result in resolver errors ([#5664](https://github.com/astral-sh/uv/pull/5664))
- Bold active nav links for accessibility ([#5673](https://github.com/astral-sh/uv/pull/5673))
- Check idempotence in packse lock scenarios ([#5485](https://github.com/astral-sh/uv/pull/5485))
- Detect python version from python project by default in `uv venv` ([#5592](https://github.com/astral-sh/uv/pull/5592))
- Drop badges from docs landing ([#5617](https://github.com/astral-sh/uv/pull/5617))
- Fix non-registry serialization for receipts ([#5668](https://github.com/astral-sh/uv/pull/5668))
- Generate CLI reference for documentation ([#5685](https://github.com/astral-sh/uv/pull/5685))
- Improve copy of console command examples ([#5397](https://github.com/astral-sh/uv/pull/5397))
- Improve the project guide ([#5626](https://github.com/astral-sh/uv/pull/5626))
- Improve the Python version concepts documentation ([#5638](https://github.com/astral-sh/uv/pull/5638))
- Improve the dependency concept documentation ([#5658](https://github.com/astral-sh/uv/pull/5658))
- Include newly-added optional dependencies in lockfile ([#5686](https://github.com/astral-sh/uv/pull/5686))
- Initialize the cache in `uv init` ([#5669](https://github.com/astral-sh/uv/pull/5669))
- Limit sync after `uv add` ([#5705](https://github.com/astral-sh/uv/pull/5705))
- Move pip-compatibility doc into pip interface section ([#5670](https://github.com/astral-sh/uv/pull/5670))
- Move settings reference to reference section ([#5689](https://github.com/astral-sh/uv/pull/5689))
- Omit the nav bar title when it has no use ([#5316](https://github.com/astral-sh/uv/pull/5316))
- Omit transitive development dependencies from workspace lockfile ([#5646](https://github.com/astral-sh/uv/pull/5646))
- Prioritize forks based on Python narrowing ([#5642](https://github.com/astral-sh/uv/pull/5642))
- Prioritize forks based on upper bounds ([#5643](https://github.com/astral-sh/uv/pull/5643))
- Prompt an early jump to the feature overview during first steps ([#5655](https://github.com/astral-sh/uv/pull/5655))
- Remove breadcrumbs for navigation ([#5676](https://github.com/astral-sh/uv/pull/5676))
- Replace `--python-preference installed` with `managed` ([#5637](https://github.com/astral-sh/uv/pull/5637))
- Set lower bounds in `uv add` ([#5688](https://github.com/astral-sh/uv/pull/5688))
- Simplify GHA `UV_SYSTEM_PYTHON` examples ([#5659](https://github.com/astral-sh/uv/pull/5659))
- Support legacy tool receipts with PEP 508 requirements ([#5679](https://github.com/astral-sh/uv/pull/5679))
- Unhide the experimental top-level commands ([#5700](https://github.com/astral-sh/uv/pull/5700))
- Use "uv" for title of index instead of "Introduction" ([#5677](https://github.com/astral-sh/uv/pull/5677))
- Use fork markers and fork preferences in resolution with lockfile ([#5481](https://github.com/astral-sh/uv/pull/5481))
- Use full requirement when serializing receipt ([#5494](https://github.com/astral-sh/uv/pull/5494))
- Use intersection rather than union for `requires-python` ([#5644](https://github.com/astral-sh/uv/pull/5644))
- `uvx` warn when no executables are available ([#5675](https://github.com/astral-sh/uv/pull/5675))

## 0.2.32

### Preview features

- Implement `uv run --directory` ([#5566](https://github.com/astral-sh/uv/pull/5566))
- Add `--isolated` support to `uv run` ([#5471](https://github.com/astral-sh/uv/pull/5471))
- Add `--no-workspace` and `--no-project` in lieu of `--isolated` ([#5465](https://github.com/astral-sh/uv/pull/5465))
- Add documentation for cache clearing ([#5517](https://github.com/astral-sh/uv/pull/5517))
- Add forks to lockfile, don't read them yet ([#5480](https://github.com/astral-sh/uv/pull/5480))
- Add links to documentation footer ([#5616](https://github.com/astral-sh/uv/pull/5616))
- Error when multiple git references are provided in `uv add` ([#5502](https://github.com/astral-sh/uv/pull/5502))
- Improvements to the project concept docs ([#5634](https://github.com/astral-sh/uv/pull/5634))
- List installed tools when no command is provided to `uv tool run` ([#5553](https://github.com/astral-sh/uv/pull/5553))
- Make `--directory` a global argument ([#5579](https://github.com/astral-sh/uv/pull/5579))
- Reframe use of `--isolated` in `tool run` ([#5470](https://github.com/astral-sh/uv/pull/5470))
- Remove `--isolated` usages from the `uv python` API ([#5468](https://github.com/astral-sh/uv/pull/5468))
- Rename more use of "lock file" to "lockfile" ([#5629](https://github.com/astral-sh/uv/pull/5629))
- Suppress resolver output by default in `uv run` and `uv tool run` ([#5580](https://github.com/astral-sh/uv/pull/5580))
- Wrap documentation at 100 characters ([#5635](https://github.com/astral-sh/uv/pull/5635))

## 0.2.31

### Preview features

- Allow `uv pip install` for unmanaged projects ([#5504](https://github.com/astral-sh/uv/pull/5504))
- Compare simplified paths in Windows exclusion tests ([#5525](https://github.com/astral-sh/uv/pull/5525))
- Respect reinstalls in cached environments ([#5499](https://github.com/astral-sh/uv/pull/5499))
- Use `hatchling` rather than implicit `setuptools` default ([#5527](https://github.com/astral-sh/uv/pull/5527))
- Use relocatable installs to support concurrency-safe cached environments ([#5509](https://github.com/astral-sh/uv/pull/5509))
- Support `--editable` installs for `uv tool` ([#5454](https://github.com/astral-sh/uv/pull/5454))
- Fix basic case of overlapping markers ([#5488](https://github.com/astral-sh/uv/pull/5488))

## 0.2.30

### Preview features

- Allow distributions to be absent in deserialization ([#5453](https://github.com/astral-sh/uv/pull/5453))
- Merge identical forks ([#5405](https://github.com/astral-sh/uv/pull/5405))
- Minor consistency fixes for code blocks ([#5437](https://github.com/astral-sh/uv/pull/5437))
- Prefer "lockfile" to "lock file" ([#5427](https://github.com/astral-sh/uv/pull/5427))
- Update documentation sections ([#5452](https://github.com/astral-sh/uv/pull/5452))
- Use `sitecustomize.py` to implement environment layering ([#5462](https://github.com/astral-sh/uv/pull/5462))
- Use stripped variants by default in Python install ([#5451](https://github.com/astral-sh/uv/pull/5451))

## 0.2.29

### Preview features

- Add PyPy finder ([#5337](https://github.com/astral-sh/uv/pull/5337))
- Add `uv init --virtual` ([#5396](https://github.com/astral-sh/uv/pull/5396))
- Allow `uv init` in unmanaged projects ([#5372](https://github.com/astral-sh/uv/pull/5372))
- Allow comments in `.python-version[s]` ([#5350](https://github.com/astral-sh/uv/pull/5350))
- Always show lock updates in `uv lock` ([#5413](https://github.com/astral-sh/uv/pull/5413))
- Improvements to the docs content ([#5426](https://github.com/astral-sh/uv/pull/5426))
- Fix blurring from nav title box shadow ([#5374](https://github.com/astral-sh/uv/pull/5374))
- Ignore Ctrl-C signals in `uv run` and `uv tool run` ([#5395](https://github.com/astral-sh/uv/pull/5395))
- Ignore hidden directories in workspace discovery ([#5408](https://github.com/astral-sh/uv/pull/5408))
- Increase padding between each nav section ([#5373](https://github.com/astral-sh/uv/pull/5373))
- Mark `--raw-sources` as conflicting with sources-specific arguments ([#5378](https://github.com/astral-sh/uv/pull/5378))
- Omit empty uv.tool.dev-dependencies on `uv init` ([#5406](https://github.com/astral-sh/uv/pull/5406))
- Omit interpreter path during `uv venv` with managed Python ([#5311](https://github.com/astral-sh/uv/pull/5311))
- Omit interpreter path from output when using managed Python ([#5313](https://github.com/astral-sh/uv/pull/5313))
- Reject Git CLI arguments with non-Git sources ([#5377](https://github.com/astral-sh/uv/pull/5377))
- Retain dependency specifier in `uv add` with sources ([#5370](https://github.com/astral-sh/uv/pull/5370))
- Show additions and removals in `uv lock` updates ([#5410](https://github.com/astral-sh/uv/pull/5410))
- Skip 'Nothing to uninstall' message when removing dangling environments ([#5382](https://github.com/astral-sh/uv/pull/5382))
- Support `requirements.txt` files in `uv tool install` and `uv tool run` ([#5362](https://github.com/astral-sh/uv/pull/5362))
- Use env variables in Github Actions docs ([#5411](https://github.com/astral-sh/uv/pull/5411))
- Use logo in documentation ([#5421](https://github.com/astral-sh/uv/pull/5421))
- Warn on `requirements.txt`-provided arguments in `uv run` et al ([#5364](https://github.com/astral-sh/uv/pull/5364))

## 0.2.28

### Preview features

- Add `requires-python` to `uv init` ([#5322](https://github.com/astral-sh/uv/pull/5322))
- Add `uv add --no-editable` ([#5246](https://github.com/astral-sh/uv/pull/5246))
- Add constraint dependencies to pyproject.toml ([#5248](https://github.com/astral-sh/uv/pull/5248))
- Add support for requirements files in `uv run` ([#4973](https://github.com/astral-sh/uv/pull/4973))
- Avoid redundant members update in `uv init` ([#5321](https://github.com/astral-sh/uv/pull/5321))
- Create member `pyproject.toml` prior to workspace discovery ([#5317](https://github.com/astral-sh/uv/pull/5317))
- Fix `uv init .` ([#5330](https://github.com/astral-sh/uv/pull/5330))
- Fix `uv init` creation of a sub-package by path ([#5247](https://github.com/astral-sh/uv/pull/5247))
- Fix colors in `uv tool run` suggestion ([#5267](https://github.com/astral-sh/uv/pull/5267))
- Improve consistency of `tool` CLI ([#5326](https://github.com/astral-sh/uv/pull/5326))
- Make tool install robust to malformed receipts ([#5305](https://github.com/astral-sh/uv/pull/5305))
- Reduce spacing between nav items ([#5310](https://github.com/astral-sh/uv/pull/5310))
- Respect exclusions in `uv init` ([#5318](https://github.com/astral-sh/uv/pull/5318))
- Store resolution options in lockfile ([#5264](https://github.com/astral-sh/uv/pull/5264))
- Use backticks in project init message ([#5302](https://github.com/astral-sh/uv/pull/5302))
- Ignores workspace when `--isolated` flag is used in `uv init` ([#5290](https://github.com/astral-sh/uv/pull/5290))
- Normalize directory names in `uv init` ([#5292](https://github.com/astral-sh/uv/pull/5292))
- Avoid project discovery in `uv python pin` if `--isolated` is provided ([#5354](https://github.com/astral-sh/uv/pull/5354))
- Show symbolic links in `uv python list` ([#5343](https://github.com/astral-sh/uv/pull/5343))
- Discover workspace from target path in `uv init` ([#5250](https://github.com/astral-sh/uv/pull/5250))
- Do not create nested workspace in `uv init`  ([#5293](https://github.com/astral-sh/uv/pull/5293))

## 0.2.27

### Preview features

- Add `--frozen` to `uv add`, `uv remove`, and `uv tree` ([#5214](https://github.com/astral-sh/uv/pull/5214))
- Add `--locked` and `--frozen` to `uv run` CLI ([#5196](https://github.com/astral-sh/uv/pull/5196))
- Add `uv tool dir --bin` to show executable directory ([#5160](https://github.com/astral-sh/uv/pull/5160))
- Add `uv tool list --show-paths` to show install paths ([#5164](https://github.com/astral-sh/uv/pull/5164))
- Add color to `python pin` CLI ([#5215](https://github.com/astral-sh/uv/pull/5215))
- Added a way to inspect installation scripts on Powershell(Windows) ([#5157](https://github.com/astral-sh/uv/pull/5157))
- Avoid TOCTOU errors in `.python-version` reads ([#5223](https://github.com/astral-sh/uv/pull/5223))
- Only show the Python installed on the system if `--python-preference only-system` is specified ([#5219](https://github.com/astral-sh/uv/pull/5219))
- Check `python pin` compatibility with `Requires-Python` ([#4989](https://github.com/astral-sh/uv/pull/4989))
- Enforce hashes in lockfile install ([#5170](https://github.com/astral-sh/uv/pull/5170))
- Fix reference to `uv run` in `uv tree` CLI ([#5216](https://github.com/astral-sh/uv/pull/5216))
- Handle universal vs. fork markers with `ResolverMarkers` ([#5099](https://github.com/astral-sh/uv/pull/5099))
- Implement `uv init` ([#4791](https://github.com/astral-sh/uv/pull/4791))
- Make Python install robust to individual failures ([#5199](https://github.com/astral-sh/uv/pull/5199))
- Make registry hashes optional in the lockfile ([#5166](https://github.com/astral-sh/uv/pull/5166))
- Merge extras in lockfile ([#5181](https://github.com/astral-sh/uv/pull/5181))
- Move integration guide docs and edit Azure integration guide ([#5117](https://github.com/astral-sh/uv/pull/5117))
- Process completed Python installs and uninstalls as a stream ([#5203](https://github.com/astral-sh/uv/pull/5203))
- Skip invalid tools in `uv tool list` ([#5156](https://github.com/astral-sh/uv/pull/5156))
- Touch-ups to tools guide ([#5202](https://github.com/astral-sh/uv/pull/5202))
- Use +- install output for Python versions ([#5201](https://github.com/astral-sh/uv/pull/5201))
- Use display representation for download error ([#5173](https://github.com/astral-sh/uv/pull/5173))
- Use specialized error message for invalid Python install / uninstall requests ([#5171](https://github.com/astral-sh/uv/pull/5171))
- Use the strongest hash in the lockfile ([#5167](https://github.com/astral-sh/uv/pull/5167))
- Write project guide ([#5195](https://github.com/astral-sh/uv/pull/5195))
- Write tools concept document ([#5207](https://github.com/astral-sh/uv/pull/5207))
- Fix reference to `projects.md` ([#5154](https://github.com/astral-sh/uv/pull/5154))
- Fixes to the settings documentation ([#5177](https://github.com/astral-sh/uv/pull/5177))
- Set exact version specifiers when resolving from lockfile ([#5193](https://github.com/astral-sh/uv/pull/5193))

## 0.2.26

### Preview features

- Indicate that `uv lock --upgrade` has updated the lock file ([#5110](https://github.com/astral-sh/uv/pull/5110))
- Sort managed Python installations by version ([#5140](https://github.com/astral-sh/uv/pull/5140))
- Support workspace to workspace path dependencies ([#4833](https://github.com/astral-sh/uv/pull/4833))
- Allow conflicting locals when forking ([#5104](https://github.com/astral-sh/uv/pull/5104))
- Rework `pyproject.toml` reformatting to respect original indentation ([#5075](https://github.com/astral-sh/uv/pull/5075))

### Documentation

- Add stubs for the project documentation ([#5135](https://github.com/astral-sh/uv/pull/5135))
- Add `settings.md` to docs ([#5091](https://github.com/astral-sh/uv/pull/5091))
- Add contributor documentation for the docs ([#5108](https://github.com/astral-sh/uv/pull/5108))
- Add reference documentation for global settings ([#5123](https://github.com/astral-sh/uv/pull/5123))
- Add reference documentation for pip settings ([#5125](https://github.com/astral-sh/uv/pull/5125))
- Add reference documentation for resolver settings ([#5122](https://github.com/astral-sh/uv/pull/5122))
- Add uv to docs Pull Request titles ([#5115](https://github.com/astral-sh/uv/pull/5115))
- Auto-merge docs PRs on release ([#5101](https://github.com/astral-sh/uv/pull/5101))
- Autogenerate possible values for enums in reference documentation ([#5137](https://github.com/astral-sh/uv/pull/5137))

## 0.2.25

### Preview features

- Add documentation for running scripts ([#4968](https://github.com/astral-sh/uv/pull/4968))
- Add guide for tools ([#4982](https://github.com/astral-sh/uv/pull/4982))
- Allow URL dependencies in tool run `--from` ([#5002](https://github.com/astral-sh/uv/pull/5002))
- Add guide for authenticating to Azure Artifacts ([#4857](https://github.com/astral-sh/uv/pull/4857))
- Improve rc file detection based on rustup ([#5026](https://github.com/astral-sh/uv/pull/5026))
- Rename `python install --force` parameter to `--reinstall` ([#4999](https://github.com/astral-sh/uv/pull/4999))
- Use lockfile to prefill resolver index ([#4495](https://github.com/astral-sh/uv/pull/4495))
- `uv tool install` hint the correct when the executable is available ([#5019](https://github.com/astral-sh/uv/pull/5019))
- `uv tool run` error messages references `uvx` when appropriate ([#5014](https://github.com/astral-sh/uv/pull/5014))
- `uvx` warns when requested executable is not provided by the package [#5071](https://github.com/astral-sh/uv/pull/5071))
- Exit with zero when `uv tool install` request is already satisfied ([#4986](https://github.com/astral-sh/uv/pull/4986))
- Respect the libc of the execution environment with `uv python list` ([#5036](https://github.com/astral-sh/uv/pull/5036))
- Update standalone Pythons to include 3.12.4 ([#5042](https://github.com/astral-sh/uv/pull/5042))
- `uv tool run` suggest valid commands when command is not found ([#4997](https://github.com/astral-sh/uv/pull/4997))
- Add Windows path updates for `uv tool` ([#5029](https://github.com/astral-sh/uv/pull/5029))
- Add a command to append uv's binary directory to PATH ([#4975](https://github.com/astral-sh/uv/pull/4975))

## 0.2.24

### Preview features

- Add Python installation guide ([#4942](https://github.com/astral-sh/uv/pull/4942))
- Add `uv python pin` ([#4950](https://github.com/astral-sh/uv/pull/4950))
- Add command-separation for Python discovery display ([#4916](https://github.com/astral-sh/uv/pull/4916))
- Avoid debug error for `uv run` with unknown Python version ([#4913](https://github.com/astral-sh/uv/pull/4913))
- Enable `--all` to uninstall all managed Pythons ([#4932](https://github.com/astral-sh/uv/pull/4932))
- Enable `--all` to uninstall all managed tools ([#4937](https://github.com/astral-sh/uv/pull/4937))
- Filter out markers based on Python requirement ([#4912](https://github.com/astral-sh/uv/pull/4912))
- Implement `uv tree` ([#4708](https://github.com/astral-sh/uv/pull/4708))
- Improve 'any' search message during `uv python install` ([#4940](https://github.com/astral-sh/uv/pull/4940))
- Lock for the duration of tool commands ([#4720](https://github.com/astral-sh/uv/pull/4720))
- Perform lock in `uv sync` by default ([#4839](https://github.com/astral-sh/uv/pull/4839))
- Reinstall and recreate environments when interpreter is removed ([#4935](https://github.com/astral-sh/uv/pull/4935))
- Respect `--isolated` in `uv python install` ([#4938](https://github.com/astral-sh/uv/pull/4938))
- Respect resolver settings in `uv remove` ([#4930](https://github.com/astral-sh/uv/pull/4930))
- Update "Python versions" documentation ([#4943](https://github.com/astral-sh/uv/pull/4943))
- Warn if tool binary directory is not on path ([#4951](https://github.com/astral-sh/uv/pull/4951))
- Avoid reparsing wheel URLs ([#4947](https://github.com/astral-sh/uv/pull/4947))
- Avoid serializing if lockfile does not change ([#4945](https://github.com/astral-sh/uv/pull/4945))

## 0.2.23

### Preview features

- Avoid creating cache directories in tool directory ([#4868](https://github.com/astral-sh/uv/pull/4868))
- Add progress bar when downloading python ([#4840](https://github.com/astral-sh/uv/pull/4840))
- Add some decoration to tool CLI ([#4865](https://github.com/astral-sh/uv/pull/4865))
- Add some text decoration to toolchain CLI ([#4882](https://github.com/astral-sh/uv/pull/4882))
- Add user-facing output to indicate PEP 723 script ([#4881](https://github.com/astral-sh/uv/pull/4881))
- Ensure Pythons are aligned in `uv python list` ([#4884](https://github.com/astral-sh/uv/pull/4884))
- Fix always-plural message in uv python install ([#4866](https://github.com/astral-sh/uv/pull/4866))
- Skip installing `--with` requirements if present in base environment ([#4879](https://github.com/astral-sh/uv/pull/4879))
- Sort dependencies before wheels and source distributions ([#4897](https://github.com/astral-sh/uv/pull/4897))
- Improve logging during resolver forking ([#4894](https://github.com/astral-sh/uv/pull/4894))

## 0.2.22

### Preview features

- Always use base interpreter for cached environments ([#4805](https://github.com/astral-sh/uv/pull/4805))
- Cache tool environments in `uv tool run` ([#4784](https://github.com/astral-sh/uv/pull/4784))
- Check hash of downloaded python toolchain ([#4806](https://github.com/astral-sh/uv/pull/4806))
- Remove incompatible wheels from `uv.lock` ([#4799](https://github.com/astral-sh/uv/pull/4799))
- `uv cache prune` removes all cached environments ([#4845](https://github.com/astral-sh/uv/pull/4845))
- Add dedicated help menu for `uvx` ([#4770](https://github.com/astral-sh/uv/pull/4770))
- Change "toolchain" to "python" ([#4735](https://github.com/astral-sh/uv/pull/4735))
- Create empty environment for `uv run --isolated` ([#4849](https://github.com/astral-sh/uv/pull/4849))
- Deduplicate when install or uninstall python ([#4841](https://github.com/astral-sh/uv/pull/4841))
- Require at least one target for toolchain uninstalls ([#4820](https://github.com/astral-sh/uv/pull/4820))
- Resolve requirements prior to nuking tool environments ([#4788](https://github.com/astral-sh/uv/pull/4788))
- Tweak installation language in toolchain install ([#4811](https://github.com/astral-sh/uv/pull/4811))
- Use already-installed tools in `uv tool run` ([#4750](https://github.com/astral-sh/uv/pull/4750))
- Use cached environments in PEP 723 execution ([#4789](https://github.com/astral-sh/uv/pull/4789))
- Use optimized versions of managed Python on Linux ([#4775](https://github.com/astral-sh/uv/pull/4775))
- Fill Python requests with platform information during automatic fetches ([#4810](https://github.com/astral-sh/uv/pull/4810))
- Remove installed python for force installation ([#4807](https://github.com/astral-sh/uv/pull/4807))
- Add tool version to list command ([#4674](https://github.com/astral-sh/uv/pull/4674))
- Add entrypoints to tool list ([#4661](https://github.com/astral-sh/uv/pull/4661))

## 0.2.21

### Preview features

- Replace tool environments on updated Python request ([#4746](https://github.com/astral-sh/uv/pull/4746))

## 0.2.20

<!-- No changes -->


## 0.2.19

### Preview features

- Remove dangling environments in `uv tool uninstall` ([#4740](https://github.com/astral-sh/uv/pull/4740))
- Respect upgrades in `uv tool install` ([#4736](https://github.com/astral-sh/uv/pull/4736))
- Add PEP 723 support to `uv run` ([#4656](https://github.com/astral-sh/uv/pull/4656))
- Add `tool dir` and `toolchain dir` commands ([#4695](https://github.com/astral-sh/uv/pull/4695))
- Omit `pythonX.Y` segment in stdlib path for managed toolchains on Windows ([#4727](https://github.com/astral-sh/uv/pull/4727))
- Add `uv toolchain uninstall` ([#4646](https://github.com/astral-sh/uv/pull/4646))
- Add `uvx` alias for `uv tool run` ([#4632](https://github.com/astral-sh/uv/pull/4632))
- Allow configuring the toolchain fetch strategy ([#4601](https://github.com/astral-sh/uv/pull/4601))
- Drop `prefer` prefix from `toolchain-preference` values ([#4602](https://github.com/astral-sh/uv/pull/4602))
- Enable projects to opt-out of workspace management ([#4565](https://github.com/astral-sh/uv/pull/4565))
- Fetch managed toolchains if necessary in `uv tool install` and `uv tool run` ([#4717](https://github.com/astral-sh/uv/pull/4717))
- Fix tool dist-info directory normalization ([#4686](https://github.com/astral-sh/uv/pull/4686))
- Lock the toolchains directory during toolchain operations ([#4733](https://github.com/astral-sh/uv/pull/4733))
- Log when we start solving a fork ([#4684](https://github.com/astral-sh/uv/pull/4684))
- Reinstall entrypoints with `--force` ([#4697](https://github.com/astral-sh/uv/pull/4697))
- Respect data scripts in `uv tool install` ([#4693](https://github.com/astral-sh/uv/pull/4693))
- Set fork solution as preference when resolving ([#4662](https://github.com/astral-sh/uv/pull/4662))
- Show dedicated message for tools with no entrypoints ([#4694](https://github.com/astral-sh/uv/pull/4694))
- Support unnamed requirements in `uv tool install` ([#4716](https://github.com/astral-sh/uv/pull/4716))

## 0.2.18

### Preview features

- Add `uv tool list` ([#4630](https://github.com/astral-sh/uv/pull/4630))
- Add `uv tool uninstall` ([#4641](https://github.com/astral-sh/uv/pull/4641))
- Add support for specifying `name@version` in `uv tool run` ([#4572](https://github.com/astral-sh/uv/pull/4572))
- Allow `uv add` to specify optional dependency groups ([#4607](https://github.com/astral-sh/uv/pull/4607))
- Allow the package spec to be passed positionally in `uv tool install` ([#4564](https://github.com/astral-sh/uv/pull/4564))
- Avoid infinite loop for cyclic installs ([#4633](https://github.com/astral-sh/uv/pull/4633))
- Indent wheels like dependencies in the lockfile ([#4582](https://github.com/astral-sh/uv/pull/4582))
- Sync all packages in a virtual workspace ([#4636](https://github.com/astral-sh/uv/pull/4636))
- Use inline table for dependencies in lockfile ([#4581](https://github.com/astral-sh/uv/pull/4581))
- Make `source` field in lock file more structured ([#4627](https://github.com/astral-sh/uv/pull/4627))

## 0.2.17

### Preview features

- Add `--extra` to `uv add` and enable fine-grained updates ([#4566](https://github.com/astral-sh/uv/pull/4566))

## 0.2.16

### Preview features

- Add `--package` argument to `uv add` and `uv remove` ([#4556](https://github.com/astral-sh/uv/pull/4556))
- Add `uv tool install` ([#4492](https://github.com/astral-sh/uv/pull/4492))
- Fallback to interpreter discovery in `uv run` ([#4549](https://github.com/astral-sh/uv/pull/4549))
- Make `uv.sources` without `--preview` non-fatal ([#4558](https://github.com/astral-sh/uv/pull/4558))
- Remove non-existent extras from lockfile ([#4479](https://github.com/astral-sh/uv/pull/4479))
- Support conflicting URL in separate forks ([#4435](https://github.com/astral-sh/uv/pull/4435))
- Automatically detect workspace packages in `uv add` ([#4557](https://github.com/astral-sh/uv/pull/4557))
- Omit `distribution.sdist` from lockfile when it is redundant ([#4528](https://github.com/astral-sh/uv/pull/4528))
- Remove `source` and `version` from lock file when unambiguous ([#4513](https://github.com/astral-sh/uv/pull/4513))
- Allow `uv lock` to read overrides from `tool.uv` (#4108) ([#4369](https://github.com/astral-sh/uv/pull/4369))

## 0.2.15

<!-- No changes -->


## 0.2.14

### Preview features

- Expose `toolchain-preference` as a CLI and configuration file option ([#4424](https://github.com/astral-sh/uv/pull/4424))
- Improve handling of command arguments in `uv run` and `uv tool run` ([#4404](https://github.com/astral-sh/uv/pull/4404))
- Add `tool.uv.sources` support for `uv add` ([#4406](https://github.com/astral-sh/uv/pull/4406))
- Use correct lock path for workspace dependencies ([#4421](https://github.com/astral-sh/uv/pull/4421))
- Filter out sibling dependencies in resolver forks ([#4415](https://github.com/astral-sh/uv/pull/4415))

## 0.2.13

### Preview features

- Add `--workspace` option to `uv add` ([#4362](https://github.com/astral-sh/uv/pull/4362))
- Ignore query errors during `uv toolchain list` ([#4382](https://github.com/astral-sh/uv/pull/4382))
- Respect `.python-version` files and fetch manged toolchains in uv project commands ([#4361](https://github.com/astral-sh/uv/pull/4361))
- Respect `.python-version` in `uv venv --preview` ([#4360](https://github.com/astral-sh/uv/pull/4360))

## 0.2.12

### Preview features

- Add `--force` option to `uv toolchain install` ([#4313](https://github.com/astral-sh/uv/pull/4313))
- Add `--no-build`, `--no-build-package`, and binary variants ([#4322](https://github.com/astral-sh/uv/pull/4322))
- Add `EXTERNALLY-MANAGED` markers to managed toolchains ([#4312](https://github.com/astral-sh/uv/pull/4312))
- Add `uv toolchain find` ([#4206](https://github.com/astral-sh/uv/pull/4206))
- Add persistent configuration for non-`pip` APIs ([#4294](https://github.com/astral-sh/uv/pull/4294))
- Add support for adding/removing development dependencies ([#4327](https://github.com/astral-sh/uv/pull/4327))
- Add support for listing system toolchains ([#4172](https://github.com/astral-sh/uv/pull/4172))
- Add support for toolchain requests by key ([#4332](https://github.com/astral-sh/uv/pull/4332))
- Allow multiple toolchains to be requested in `uv toolchain install` ([#4334](https://github.com/astral-sh/uv/pull/4334))
- Fix relative and absolute path handling in lockfiles ([#4266](https://github.com/astral-sh/uv/pull/4266))
- Load configuration options from workspace root ([#4295](https://github.com/astral-sh/uv/pull/4295))
- Omit project name from workspace errors ([#4299](https://github.com/astral-sh/uv/pull/4299))
- Read Python version files during toolchain installs ([#4335](https://github.com/astral-sh/uv/pull/4335))
- Remove extraneous installations in `uv sync` by default ([#4366](https://github.com/astral-sh/uv/pull/4366))
- Respect `requires-python` in `uv lock` ([#4282](https://github.com/astral-sh/uv/pull/4282))
- Respect workspace-wide `requires-python` in interpreter selection ([#4298](https://github.com/astral-sh/uv/pull/4298))
- Support unnamed requirements in `uv add` ([#4326](https://github.com/astral-sh/uv/pull/4326))
- Use portable slash paths in lockfile ([#4324](https://github.com/astral-sh/uv/pull/4324))
- Use registry URL for fetching source distributions from lockfile ([#4280](https://github.com/astral-sh/uv/pull/4280))
- `uv sync --no-clean` ([#4367](https://github.com/astral-sh/uv/pull/4367))
- Filter dependencies by tracking markers on resolver forks ([#4339](https://github.com/astral-sh/uv/pull/4339))
- Use `Requires-Python` to filter dependencies during universal resolution ([#4273](https://github.com/astral-sh/uv/pull/4273))

## 0.2.11

### Preview features

- Add changelog for preview changes ([#4251](https://github.com/astral-sh/uv/pull/4251))
- Allow direct URLs for dev dependencies ([#4233](https://github.com/astral-sh/uv/pull/4233))
- Create temporary environments in dedicated cache bucket ([#4223](https://github.com/astral-sh/uv/pull/4223))
- Improve output when an older toolchain version is already installed ([#4248](https://github.com/astral-sh/uv/pull/4248))
- Initial implementation of `uv add` and `uv remove` ([#4193](https://github.com/astral-sh/uv/pull/4193))
- Refactor project interpreter request for `requires-python` specifiers ([#4216](https://github.com/astral-sh/uv/pull/4216))
- Replace `toolchain fetch` with `toolchain install` ([#4228](https://github.com/astral-sh/uv/pull/4228))
- Support locking relative paths ([#4205](https://github.com/astral-sh/uv/pull/4205))
- Warn when 'requires-python' does not include a lower bound ([#4234](https://github.com/astral-sh/uv/pull/4234))

## 0.2.10

### Preview features

- Add `uv toolchain install` ([#4164](https://github.com/astral-sh/uv/pull/4164))
- Add `uv toolchain list` ([#4163](https://github.com/astral-sh/uv/pull/4163))
- Add extra and dev dependency validation to lockfile ([#4112](https://github.com/astral-sh/uv/pull/4112))
- Add markers to edges rather than distributions ([#4166](https://github.com/astral-sh/uv/pull/4166))
- Cap `Requires-Python` comparisons at the patch version ([#4150](https://github.com/astral-sh/uv/pull/4150))
- Do not create a virtual environment when locking ([#4147](https://github.com/astral-sh/uv/pull/4147))
- Don't panic with invalid wheel source ([#4191](https://github.com/astral-sh/uv/pull/4191))
- Fetch managed toolchains in `uv run` ([#4143](https://github.com/astral-sh/uv/pull/4143))
- Fix PEP 508 link in preview doc `specifying_dependencies` ([#4158](https://github.com/astral-sh/uv/pull/4158))
- Ignore tags in universal resolution ([#4174](https://github.com/astral-sh/uv/pull/4174))
- Implement `Toolchain::find_or_fetch` and use in `uv venv --preview` ([#4138](https://github.com/astral-sh/uv/pull/4138))
- Lock all packages in workspace ([#4016](https://github.com/astral-sh/uv/pull/4016))
- Recreate project environment if `--python` or `requires-python` doesn't match ([#3945](https://github.com/astral-sh/uv/pull/3945))
- Respect `--find-links` in `lock` and `sync` ([#4183](https://github.com/astral-sh/uv/pull/4183))
- Set `--dev` to default for `uv run` and `uv sync` ([#4118](https://github.com/astral-sh/uv/pull/4118))
- Track `Markers` via a PubGrub package variant ([#4123](https://github.com/astral-sh/uv/pull/4123))
- Use union of `requires-python` in workspace ([#4041](https://github.com/astral-sh/uv/pull/4041))
- make universal resolver fork only when markers are disjoint ([#4135](https://github.com/astral-sh/uv/pull/4135))

## 0.2.9

### Preview features

- Add support for development dependencies ([#4036](https://github.com/astral-sh/uv/pull/4036))
- Avoid enforcing distribution ID uniqueness for extras ([#4104](https://github.com/astral-sh/uv/pull/4104))
- Ignore upper-bounds on `Requires-Python` ([#4086](https://github.com/astral-sh/uv/pull/4086))

## 0.2.8

### Preview features

- Default to current Python minor if `Requires-Python` is absent ([#4070](https://github.com/astral-sh/uv/pull/4070))
- Enforce `Requires-Python` when syncing ([#4068](https://github.com/astral-sh/uv/pull/4068))
- Track supported Python range in lockfile ([#4065](https://github.com/astral-sh/uv/pull/4065))

## 0.2.7

### Preview features

- Fix a bug where no warning is output when parsing of workspace settings fails. ([#4014](https://github.com/astral-sh/uv/pull/4014))
- Normalize extras in lockfile ([#3958](https://github.com/astral-sh/uv/pull/3958))
- Respect `Requires-Python` in universal resolution ([#3998](https://github.com/astral-sh/uv/pull/3998))

## 0.2.6

### Preview features

- Add `uv run --package` ([#3864](https://github.com/astral-sh/uv/pull/3864))
- Add index URL parameters to Project CLI ([#3984](https://github.com/astral-sh/uv/pull/3984))
- Avoid re-adding solutions to forked state ([#3967](https://github.com/astral-sh/uv/pull/3967))
- Draft for user docs for workspaces ([#3866](https://github.com/astral-sh/uv/pull/3866))
- Include all extras when generating lockfile ([#3912](https://github.com/astral-sh/uv/pull/3912))
- Remove unstable uv lock from pip interface ([#3970](https://github.com/astral-sh/uv/pull/3970))
- Respect resolved Git SHAs in `uv lock` ([#3956](https://github.com/astral-sh/uv/pull/3956))
- Use lockfile in `uv run` ([#3894](https://github.com/astral-sh/uv/pull/3894))
- Use lockfile versions as resolution preferences ([#3921](https://github.com/astral-sh/uv/pull/3921))
- Use universal resolution in `uv lock` ([#3969](https://github.com/astral-sh/uv/pull/3969))

## 0.2.5

### Preview features

- Add context to failed `uv tool run` ([#3882](https://github.com/astral-sh/uv/pull/3882))
- Add persistent storage of installed toolchains ([#3797](https://github.com/astral-sh/uv/pull/3797))
- Gate discovery of managed toolchains with preview ([#3835](https://github.com/astral-sh/uv/pull/3835))
- Initial workspace support ([#3705](https://github.com/astral-sh/uv/pull/3705))
- Move editable discovery behind `--preview` for now ([#3884](https://github.com/astral-sh/uv/pull/3884))

## 0.2.4

<!-- No changes -->


## 0.2.3

### Preview features

- Allow specification of additional requirements in `uv tool run` ([#3678](https://github.com/astral-sh/uv/pull/3678))

## 0.2.2

<!-- No changes -->


## 0.2.1

### Preview features

- Allow users to specify a custom source package to `uv tool run` ([#3677](https://github.com/astral-sh/uv/pull/3677))

## 0.2.0

### Preview features

- Add initial implementation of `uv tool run` ([#3657](https://github.com/astral-sh/uv/pull/3657))
- Add offline support to `uv tool run` and `uv run` ([#3676](https://github.com/astral-sh/uv/pull/3676))
- Better error message for `uv run` failures ([#3691](https://github.com/astral-sh/uv/pull/3691))
- Discover workspaces without using them in resolution ([#3585](https://github.com/astral-sh/uv/pull/3585))
- Support editables in `uv sync` ([#3692](https://github.com/astral-sh/uv/pull/3692))
- Track editable requirements in lockfile ([#3725](https://github.com/astral-sh/uv/pull/3725))

## 0.1.45

### Preview features

- Add direct URL conversion to lockfile ([#3633](https://github.com/astral-sh/uv/pull/3633))
- Add hashes and versions to all distributions ([#3589](https://github.com/astral-sh/uv/pull/3589))
- Add local path conversions from lockfile ([#3609](https://github.com/astral-sh/uv/pull/3609))
- Add missing `"directory"` branch in source match ([#3608](https://github.com/astral-sh/uv/pull/3608))
- Add registry file size to lockfile ([#3652](https://github.com/astral-sh/uv/pull/3652))
- Add registry source distribution support to lockfile ([#3649](https://github.com/astral-sh/uv/pull/3649))
- Refactor editables for supporting them in bluejay commands ([#3639](https://github.com/astral-sh/uv/pull/3639))
- Rename `sourcedist` to `sdist` in lockfile ([#3590](https://github.com/astral-sh/uv/pull/3590))
- Respect installed packages in `uv run` ([#3603](https://github.com/astral-sh/uv/pull/3603))
- Support lossless serialization for Git dependencies in lockfile ([#3630](https://github.com/astral-sh/uv/pull/3630))

## 0.1.44

<!-- No changes -->


## 0.1.43

### Preview features

- Create virtualenv if it doesn't exist in project API ([#3499](https://github.com/astral-sh/uv/pull/3499))
- Discover `uv run` projects hierarchically ([#3494](https://github.com/astral-sh/uv/pull/3494))
- Read and write `uv.lock` based on project root ([#3497](https://github.com/astral-sh/uv/pull/3497))
- Read package name from `pyproject.toml` in `uv run` ([#3496](https://github.com/astral-sh/uv/pull/3496))
- Rebrand workspace API as project API ([#3489](https://github.com/astral-sh/uv/pull/3489))

## 0.1.42

### Preview features

- Use environment layering for `uv run --with` ([#3447](https://github.com/astral-sh/uv/pull/3447))
- Warn when missing minimal bounds when using `tool.uv.sources` ([#3452](https://github.com/astral-sh/uv/pull/3452))

## 0.1.41

<!-- No changes -->


## 0.1.40

### Preview features

- Add basic `tool.uv.sources` support ([#3263](https://github.com/astral-sh/uv/pull/3263))
- Improve non-git error message ([#3403](https://github.com/astral-sh/uv/pull/3403))
- Preserve given for `tool.uv.sources` paths ([#3412](https://github.com/astral-sh/uv/pull/3412))
- Restore verbatim in error message ([#3402](https://github.com/astral-sh/uv/pull/3402))
- Use preview mode for tool.uv.sources ([#3277](https://github.com/astral-sh/uv/pull/3277))
- Use top-level `--isolated` for `uv run` ([#3431](https://github.com/astral-sh/uv/pull/3431))
- add basic "install from lock file" operation ([#3340](https://github.com/astral-sh/uv/pull/3340))
- uv-resolver: add initial version of universal lock file format ([#3314](https://github.com/astral-sh/uv/pull/3314))
