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

        # Extract all `pub struct` declarations from the file.
        members = re.findall(r'pub struct (\w+)', contents)
        seen.update(members)

        # Extract all `pub enum` declarations from the file.
        members = re.findall(r'pub enum (\w+)', contents)
        seen.update(members)

    # Iterate over all `.rs` files in the current directory, recursively.
    for file in Path('.').rglob('*.rs'):
        with open(file, 'r') as f:
            contents = f.read()

        # Ex) `Foo::bar`
        calls = re.findall(r'(\w+)::', contents)

        for call in calls:
            seen.discard(call)

        # Ex) `Foo {`
        calls = re.findall(r'(\w+) {', contents)

        for call in calls:
            seen.discard(call)

    # Print all unused structs.
    for function in seen:
        # Ignore PyO3 functions.
        if function.startswith("py_"):
            continue

        # Ignore dunders.
        if function.startswith("__"):
            continue

        print(function)
