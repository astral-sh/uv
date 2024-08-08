@REM Copyright (c) 2020-202x The virtualenv developers
@REM
@REM Permission is hereby granted, free of charge, to any person obtaining
@REM a copy of this software and associated documentation files (the
@REM "Software"), to deal in the Software without restriction, including
@REM without limitation the rights to use, copy, modify, merge, publish,
@REM distribute, sublicense, and/or sell copies of the Software, and to
@REM permit persons to whom the Software is furnished to do so, subject to
@REM the following conditions:
@REM
@REM The above copyright notice and this permission notice shall be
@REM included in all copies or substantial portions of the Software.
@REM
@REM THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND,
@REM EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF
@REM MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE AND
@REM NONINFRINGEMENT. IN NO EVENT SHALL THE AUTHORS OR COPYRIGHT HOLDERS BE
@REM LIABLE FOR ANY CLAIM, DAMAGES OR OTHER LIABILITY, WHETHER IN AN ACTION
@REM OF CONTRACT, TORT OR OTHERWISE, ARISING FROM, OUT OF OR IN CONNECTION
@REM WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE SOFTWARE.

@for %%i in ("{{ VIRTUAL_ENV_DIR }}") do @set "VIRTUAL_ENV=%%~fi"

@set "VIRTUAL_ENV_PROMPT={{ VIRTUAL_PROMPT }}"
@if NOT DEFINED VIRTUAL_ENV_PROMPT (
    @for %%d in ("%VIRTUAL_ENV%") do @set "VIRTUAL_ENV_PROMPT=%%~nxd"
)

@if defined _OLD_VIRTUAL_PROMPT (
    @set "PROMPT=%_OLD_VIRTUAL_PROMPT%"
) else (
    @if not defined PROMPT (
        @set "PROMPT=$P$G"
    )
    @if not defined VIRTUAL_ENV_DISABLE_PROMPT (
        @set "_OLD_VIRTUAL_PROMPT=%PROMPT%"
    )
)
@if not defined VIRTUAL_ENV_DISABLE_PROMPT (
    @set "PROMPT=(%VIRTUAL_ENV_PROMPT%) %PROMPT%"
)

@REM Don't use () to avoid problems with them in %PATH%
@if defined _OLD_VIRTUAL_PYTHONHOME @goto ENDIFVHOME
    @set "_OLD_VIRTUAL_PYTHONHOME=%PYTHONHOME%"
:ENDIFVHOME

@set PYTHONHOME=

@REM if defined _OLD_VIRTUAL_PATH (
@if not defined _OLD_VIRTUAL_PATH @goto ENDIFVPATH1
    @set "PATH=%_OLD_VIRTUAL_PATH%"
:ENDIFVPATH1
@REM ) else (
@if defined _OLD_VIRTUAL_PATH @goto ENDIFVPATH2
    @set "_OLD_VIRTUAL_PATH=%PATH%"
:ENDIFVPATH2

@set "PATH=%VIRTUAL_ENV%\{{ BIN_NAME }};%PATH%"
