use path
use edit

var venv-path = (path:join '{{ VIRTUAL_ENV_DIR }}' '{{ BIN_NAME }}')

var paths-bak = $paths
set paths = [venv-path $@paths]

set-env VIRTUAL_ENV '{{ VIRTUAL_ENV_DIR }}'
set-env VIRTUAL_ENV_PROMPT '{{ VIRTUAL_PROMPT }}'

edit:add-var deactivate~ {
  set paths = $paths-bak
  unset-env VIRTUAL_ENV
  unset-env VIRTUAL_ENV_PROMPT
  edit:del-var deactivate~
}
