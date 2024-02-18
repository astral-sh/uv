# Copyright (c) 2020-202x The virtualenv developers
#
# Permission is hereby granted, free of charge, to any person obtaining
# a copy of this software and associated documentation files (the
# "Software"), to deal in the Software without restriction, including
# without limitation the rights to use, copy, modify, merge, publish,
# distribute, sublicense, and/or sell copies of the Software, and to
# permit persons to whom the Software is furnished to do so, subject to
# the following conditions:
#
# The above copyright notice and this permission notice shall be
# included in all copies or substantial portions of the Software.
#
# THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND,
# EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF
# MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE AND
# NONINFRINGEMENT. IN NO EVENT SHALL THE AUTHORS OR COPYRIGHT HOLDERS BE
# LIABLE FOR ANY CLAIM, DAMAGES OR OTHER LIABILITY, WHETHER IN AN ACTION
# OF CONTRACT, TORT OR OTHERWISE, ARISING FROM, OUT OF OR IN CONNECTION
# WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE SOFTWARE.

# virtualenv activation module
# Activate with `overlay use activate.nu`
# Deactivate with `deactivate`, as usual
#
# To customize the overlay name, you can call `overlay use activate.nu as foo`,
# but then simply `deactivate` won't work because it is just an alias to hide
# the "activate" overlay. You'd need to call `overlay hide foo` manually.

export-env {
    def is-string [x] {
        ($x | describe) == 'string'
    }

    def has-env [...names] {
        $names | each {|n|
            $n in $env
        } | all {|i| $i == true}
    }

    # Emulates a `test -z`, but btter as it handles e.g 'false'
    def is-env-true [name: string] {
      if (has-env $name) {
        # Try to parse 'true', '0', '1', and fail if not convertible
        let parsed = (do -i { $env | get $name | into bool })
        if ($parsed | describe) == 'bool' {
          $parsed
        } else {
          not ($env | get -i $name | is-empty)
        }
      } else {
        false
      }
    }

    let virtual_env = '{{ VIRTUAL_ENV_DIR }}'
    let bin = '{{ BIN_NAME }}'

    let is_windows = ($nu.os-info.family) == 'windows'
    let path_name = (if (has-env 'Path') {
            'Path'
        } else {
            'PATH'
        }
    )

    let venv_path = ([$virtual_env $bin] | path join)
    let new_path = ($env | get $path_name | prepend $venv_path)

    # If there is no default prompt, then use the env name instead
    let virtual_env_prompt = (if ('{{ VIRTUAL_PROMPT }}' | is-empty) {
        ($virtual_env | path basename)
    } else {
        '{{ VIRTUAL_PROMPT }}'
    })

    let new_env = {
        $path_name         : $new_path
        VIRTUAL_ENV        : $virtual_env
        VIRTUAL_ENV_PROMPT : $virtual_env_prompt
    }

    let new_env = (if (is-env-true 'VIRTUAL_ENV_DISABLE_PROMPT') {
      $new_env
    } else {
      # Creating the new prompt for the session
      let virtual_prefix = $'(char lparen)($virtual_env_prompt)(char rparen) '

      # Back up the old prompt builder
      let old_prompt_command = (if (has-env 'PROMPT_COMMAND') {
              $env.PROMPT_COMMAND
          } else {
              ''
        })

      let new_prompt = (if (has-env 'PROMPT_COMMAND') {
          if 'closure' in ($old_prompt_command | describe) {
              {|| $'($virtual_prefix)(do $old_prompt_command)' }
          } else {
              {|| $'($virtual_prefix)($old_prompt_command)' }
          }
      } else {
          {|| $'($virtual_prefix)' }
      })

      $new_env | merge {
        PROMPT_COMMAND      : $new_prompt
        VIRTUAL_PREFIX      : $virtual_prefix
      }
    })

    # Environment variables that will be loaded as the virtual env
    load-env $new_env
}

export alias pydoc = python -m pydoc
export alias deactivate = overlay hide activate
