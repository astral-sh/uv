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

@set VIRTUAL_ENV=
@set VIRTUAL_ENV_PROMPT=

@REM Don't use () to avoid problems with them in %PATH%
@if not defined _OLD_VIRTUAL_PROMPT @goto ENDIFVPROMPT
    @set "PROMPT=%_OLD_VIRTUAL_PROMPT%"
    @set _OLD_VIRTUAL_PROMPT=
:ENDIFVPROMPT

@if not defined _OLD_VIRTUAL_PYTHONHOME @goto ENDIFVHOME
    @set "PYTHONHOME=%_OLD_VIRTUAL_PYTHONHOME%"
    @set _OLD_VIRTUAL_PYTHONHOME=
:ENDIFVHOME

@if not defined _OLD_VIRTUAL_PATH @goto ENDIFVPATH
    @set "PATH=%_OLD_VIRTUAL_PATH%"
    @set _OLD_VIRTUAL_PATH=
:ENDIFVPATH