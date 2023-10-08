import json
import sys
from platform import python_version


def main():
    data = {
        "base_exec_prefix": sys.base_exec_prefix,
        "base_prefix": sys.base_prefix,
        "major": sys.version_info.major,
        "minor": sys.version_info.minor,
        "python_version": python_version(),
    }
    print(json.dumps(data))


if __name__ == "__main__":
    main()
