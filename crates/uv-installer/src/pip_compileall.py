"""
Based on
https://github.com/pypa/pip/blob/3820b0e52c7fed2b2c43ba731b718f316e6816d1/src/pip/_internal/operations/install/wheel.py#L612-L623

pip silently just swallows all pyc compilation errors, but `python -m compileall` does
not have such a flag, so we adapt the pip code. This is relevant e.g. for
`debugpy-1.5.1-cp38-cp38-manylinux_2_5_x86_64.manylinux1_x86_64.manylinux_2_12_x86_64.manylinux2010_x86_64`,
which contains some vendored python 2 code which fails to compile
"""

import compileall
import sys
import warnings

with warnings.catch_warnings():
    warnings.filterwarnings("ignore")
    # In rust, we give one line per file to compile
    for path in sys.stdin:
        # Remove trailing newline
        path = path.strip()
        if not path:
            continue
        # Unlike pip, we set quiet=2, so we don't have to capture stdout
        # I'd like to show those errors, but given that pip thinks that's totally fine
        # we can't really change that
        success = compileall.compile_file(path, force=True, quiet=2)
        # We're ready for the next file
        print(path)
