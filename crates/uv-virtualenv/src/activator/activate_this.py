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

"""
Activate virtualenv for current interpreter:

import runpy
runpy.run_path(this_file)

This can be used when you must use an existing Python interpreter, not the virtualenv bin/python.
"""  # noqa: D415

from __future__ import annotations

import os
import site
import sys

try:
    abs_file = os.path.abspath(__file__)
except NameError as exc:
    msg = "You must use import runpy; runpy.run_path(this_file)"
    raise AssertionError(msg) from exc

bin_dir = os.path.dirname(abs_file)
base = bin_dir[: -len("{{ BIN_NAME }}") - 1]  # strip away the bin part from the __file__, plus the path separator

# prepend bin to PATH (this file is inside the bin directory)
os.environ["PATH"] = os.pathsep.join([bin_dir, *os.environ.get("PATH", "").split(os.pathsep)])
os.environ["VIRTUAL_ENV"] = base  # virtual env is right above bin directory
os.environ["VIRTUAL_ENV_PROMPT"] = "{{ VIRTUAL_PROMPT }}" or os.path.basename(base)  # noqa: SIM222

# add the virtual environments libraries to the host python import mechanism
prev_length = len(sys.path)
for lib in "{{ RELATIVE_SITE_PACKAGES }}".split(os.pathsep):
    path = os.path.realpath(os.path.join(bin_dir, lib))
    site.addsitedir(path)
sys.path[:] = sys.path[prev_length:] + sys.path[0:prev_length]

sys.real_prefix = sys.prefix
sys.prefix = base
