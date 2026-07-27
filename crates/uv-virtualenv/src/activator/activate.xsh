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

# This file must be used with "source bin/activate.xsh" *from xonsh*.
# You cannot run it directly.


class _VirtualEnvActivator:
    """xonsh activation for virtualenv."""

    # Stashed in _OLD_VIRTUAL_{name} when the variable was not set before
    # activation. deactivate treats this as "unset" rather than "restore to
    # this string".
    _UNSET_SENTINEL = "__virtualenv_was_not_set__"

    def __init__(self):
        from os.path import dirname, realpath

        self.env = __xonsh__.env
        self.embedded_virtual_env = {{ VIRTUAL_ENV_DIR }}
        self.embedded_virtual_prompt = {{ VIRTUAL_PROMPT_LITERAL }}
        self.embedded_bin_name = {{ BIN_NAME_LITERAL }}
        self.managed_vars = ("PATH", "PYTHONHOME")

    def _backup_name(self, name):
        return f"_OLD_VIRTUAL_{name}"

    def _save(self, name):
        backup = self._backup_name(name)
        self.env[backup] = self.env[name] if name in self.env else self._UNSET_SENTINEL

    def _override(self, name, value):
        self._save(name)
        self.env[name] = value

    def _drop(self, name):
        self._save(name)
        self.env.pop(name, None)

    def register_pydoc(self):
        aliases["pydoc"] = ["python", "-m", "pydoc"]

    def unregister_pydoc(self):
        aliases.pop("pydoc", None)

    def activate(self):
        from os.path import basename, join

        aliases["deactivate"] = self.deactivate
        self.deactivate(["nondestructive"]) # wipe any stale state from a prior activation

        $VIRTUAL_ENV = self.embedded_virtual_env
        $VIRTUAL_ENV_PROMPT = self.embedded_virtual_prompt or basename($VIRTUAL_ENV)

        self._override("PATH", [join($VIRTUAL_ENV, self.embedded_bin_name), *$PATH])
        self._drop("PYTHONHOME")
        self.register_pydoc()

    def deactivate(self, args=None):
        for name in self.managed_vars:
            backup = self._backup_name(name)
            if backup not in self.env:
                continue
            previous = self.env[backup]
            del self.env[backup]
            if previous == self._UNSET_SENTINEL:
                self.env.pop(name, None)
            else:
                self.env[name] = previous
        for name in ("VIRTUAL_ENV", "VIRTUAL_ENV_PROMPT"):
            self.env.pop(name, None)
        self.unregister_pydoc()
        if args is None or "nondestructive" not in args:
            del aliases["deactivate"]
            try:
                del __xonsh__.xontrib.virtualenv
            except AttributeError:
                pass


if not hasattr(__xonsh__, "xontrib"):
    __xonsh__.xontrib = __xonsh__.imp.types.SimpleNamespace()
__xonsh__.xontrib.virtualenv = _VirtualEnvActivator()
__xonsh__.xontrib.virtualenv.activate()
