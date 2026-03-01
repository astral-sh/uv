## Scope

This proposal contemplates a truly minimal MVP with the intention of getting the core feature
shipped soon so any additional features/discussions can be based on actual usage patterns/needs.


## Definitions

* `$cache_dir` represents the directory where uv stores it's cache.  Any mechanisms currently in
   place for setting the directory, e.g. `UV_CACHE_DIR` and `XDG_CACHE_DIR`, remain unchanged.
* `$venv_cache_dir`: `$cache_dir/venvs-v0`
* `$uv_venv_dir`: the file system path to UV's venv location, regardless of how it is determined

## Proposal

1. Document this feature as a "preview" for ~six months to make it easier to facilitate backwards
  incompatible changes should real-world usage patterns indicate they are needed.
   - `--preview-features venvs-in-cache` to silence warning
1. Application
   - Only applies to project based venvs
   - Does NOT apply to `uv venv` invocations where a PATH is provided
1. venv storage location:
   - a cache bucket dedicated to venvs: `$venv_cache_dir`
   - customizable: no
1. Trigger centralized venv storage when:
   - `UV_VENVS_IN_CACHE`: is set to any value;
   - OR `venvs-in-cache` setting is `true`
1. Venv dir name calculation
   - [ChatGTP input on this
     question](https://chatgpt.com/share/68766176-ca84-8010-84a3-b8a3d21f4b32), I'm following it's
     recommendation
   - Human‑readable slug + short hash (pseudocode below)
     ```
      slug = slugify(basename(project_root))
      hash = sha256(project_root)[0:8]
      venv_dir = $venv_cache_dir/{slug}-{hash}
     ```
1. Place text file in the venv which contains the path to the uv project the venv was created for:
   - Path: `$venv_dir/uv-project-path.txt`
   - Contents example: `/home/joe/projects/foo`
   - Note: the directory is the project directory, not the path to the `pyproject.toml` or `uv.toml`
1. Add `uv venv --dir` to output the absolute normlized path to the current venv location
   - This is not limited to the venv-in-cache feature
   - E.g.: `print($uv_venv_dir)`
1. Cache cleanup
   - `uv cache clean` removes all venvs


## Out of Scope / Rejections

1. We will not support full-paths inside the `$cache_dir/venvs-v0`
   - Avoid possibility of hitting path length limits
   - Doesn't match what uv currently does
   - "Flat is better than nested"
2. Using a symlink in the venv to point back to the project directory
   - Windows doesn't have a good symlink solution
3. Treating venv's as non-transient and/or sharable across projects
   - While shared venvs could be a valid use case, this proposal is concerned with venvs dedicated
     to a single project and created as a result of the project's `pyproject.toml` configuration.
   - venvs are assumed to be easily and quickly created as needed and, as such, the accidental
     deletion of a venv is inconsequential since it will be recreated by any uv command that
     needs it.
4. Defer "smarter" mechanisms for cache cleaning to post-MVP
   - E.g. `uv cache prune` could remove venvs whose projects are missing.
5. IDE Discovery method
   - Assuming users that want centralized venvs will be responsible for configuring their IDE for
     the location of the virtualenv
   - FWIW, I do this by by activating the venv first and then `code .` in the project directory.  VS
     Code picks up the correct location from `$VIRTUAL_ENV` and, I'm assuming, a lot of other IDEs
     will as well.
   - Other methods for IDE discovery deferred
6. venv path templating
   - It's possible the implementation for this could piggy back on UV_PROJECT_ENVIRONMENT
     templating.  See: https://github.com/astral-sh/uv/pull/14937
   - Deferring this feature as it would necessitate discussion of templating variables supported
     as well as ENV and uv settings naming/usage which isn't necessary for an MVP.
