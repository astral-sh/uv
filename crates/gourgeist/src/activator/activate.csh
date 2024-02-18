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

# This file must be used with "source bin/activate.csh" *from csh*.
# You cannot run it directly.
# Created by Davide Di Blasi <davidedb@gmail.com>.

set newline='\
'

alias deactivate 'test $?_OLD_VIRTUAL_PATH != 0 && setenv PATH "$_OLD_VIRTUAL_PATH:q" && unset _OLD_VIRTUAL_PATH; rehash; test $?_OLD_VIRTUAL_PROMPT != 0 && set prompt="$_OLD_VIRTUAL_PROMPT:q" && unset _OLD_VIRTUAL_PROMPT; unsetenv VIRTUAL_ENV; unsetenv VIRTUAL_ENV_PROMPT; test "\!:*" != "nondestructive" && unalias deactivate && unalias pydoc'

# Unset irrelevant variables.
deactivate nondestructive

setenv VIRTUAL_ENV '{{ VIRTUAL_ENV_DIR }}'

set _OLD_VIRTUAL_PATH="$PATH:q"
setenv PATH "$VIRTUAL_ENV:q/{{ BIN_NAME }}:$PATH:q"



if ('{{ VIRTUAL_PROMPT }}' != "") then
    setenv VIRTUAL_ENV_PROMPT '{{ VIRTUAL_PROMPT }}'
else
    setenv VIRTUAL_ENV_PROMPT "$VIRTUAL_ENV:t:q"
endif

if ( $?VIRTUAL_ENV_DISABLE_PROMPT ) then
    if ( $VIRTUAL_ENV_DISABLE_PROMPT == "" ) then
        set do_prompt = "1"
    else
        set do_prompt = "0"
    endif
else
    set do_prompt = "1"
endif

if ( $do_prompt == "1" ) then
    # Could be in a non-interactive environment,
    # in which case, $prompt is undefined and we wouldn't
    # care about the prompt anyway.
    if ( $?prompt ) then
        set _OLD_VIRTUAL_PROMPT="$prompt:q"
        if ( "$prompt:q" =~ *"$newline:q"* ) then
            :
        else
            set prompt = '('"$VIRTUAL_ENV_PROMPT:q"') '"$prompt:q"
        endif
    endif
endif

unset env_name
unset do_prompt

alias pydoc python -m pydoc

rehash
