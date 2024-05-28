import re
from pathlib import Path

seen = set()

if __name__ == '__main__':
    # Iterate over all `.rs` files in the current directory, recursively.
    for file in Path('.').rglob('*.rs'):
        # Open the file for reading.
        with open(file, 'r') as f:
            # Read the file contents.
            contents = f.read()

        # Extract all `pub fn` declarations from the file.
        functions = re.findall(r'pub fn (\w+)', contents)

        # Add all function names to the `seen` set.
        seen.update(functions)

    # Iterate over all `.rs` files in the current directory, recursively.
    for file in Path('.').rglob('*.rs'):
        # Open the file for reading.
        with open(file, 'r') as f:
            # Read the file contents.
            contents = f.read()

        # Ex) `foo::bar()`
        calls = re.findall(r'::(\w+)\(', contents)

        for call in calls:
            seen.discard(call)

        # Ex) `foo.bar()`
        calls = re.findall(r'\.(\w+)\(', contents)

        for call in calls:
            seen.discard(call)

        # Ex) `baz()` (but not `fn baz()`)
        calls = re.findall(r'(?<!fn )(\w+)\(', contents)

        for call in calls:
            seen.discard(call)

        # Ex) `(bar())`
        calls = re.findall(r'\((\w+)\(', contents)

        for call in calls:
            seen.discard(call)

        # Ex) `(foo::bar)
        calls = re.findall(r'(\w+)\)', contents)

        for call in calls:
            seen.discard(call)

    # Print all unused functions.
    for function in seen:
        # Ignore PyO3 functions.
        if function.startswith("py_"):
            continue

        # Ignore dunders.
        if function.startswith("__"):
            continue

        print(function)
