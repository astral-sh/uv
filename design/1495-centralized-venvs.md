I'm going to focus on a truly minimal MVP, which will drive my thinking on the design.

1. Application
   - Only applies to project based venvs
   - Does NOT apply to `uv venv` invocations where a PATH is provided
1. venv storage location:
   - a cache bucket, e.g. `$XDG_CACHE_HOME/uv/venvs-v0`
   - customizable: no
1. Trigger centralized venv storage when:
   - `UV_VENVS_IN_CACHE`: is set to any value;
   - OR `venvs-in-cache` setting is `true`
1. venv dir name calculation
   - [ChatGTP input on this question](https://chatgpt.com/share/68766176-ca84-8010-84a3-b8a3d21f4b32), I'm following it's recommendation
   - Human‑readable slug + short hash (pseudocode below)
   - `slug = slugify(basename(project_root))`
   - `hash = sha256(project_root)[0:8]`
   - `venv_dir = $XDG_CACHE_HOME/uv/venvs-v0/{slug}-{hash}`
   - One CON of this method is that, without additional work, there is no way to look at the venv and find the location of it's project.  This affects cache cleanup (see below).
     - I believe this is an acceptable CON as a future enhancement could place a file in the venv indicating the path to the project
1. Add `uv venv --dir` to output full path to venv location, e.g. continuing from the above pseudocode: `print(venv_dir)`
1. Cache cleanup
   - `uv cache clean` removes all venvs
   - Defer "smarter" mechanisms for cache cleaning to post-MVP.  E.g. `uv cache prune` could remove venvs whose projects are missing.
1. IDE Discovery method
   - Assuming users that want centralized venvs will be responsible for configuring their IDE for the location of the virtualenv
   - FWIW, I do this by by activating the venv first and then `code .` in the project directory.  VS Code picks up the correct location from `$VIRTUAL_ENV` and, I'm assuming, a lot of other IDEs will as well.
   - Other methods for IDE discovery deferred until those mechanisms (e.g. path in .venv file) are finalized/working

Defer all other considerations to post-MVP.
