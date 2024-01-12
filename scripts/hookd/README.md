# hookd

A daemon process for PEP 517 build hook requests.


## Example
```
PYTHONPATH=scripts/hookd/backends ./scripts/hookd/hookd.py < scripts/hookd/example.in
```

## Command line


Build hooks should be run from the source tree of the project.

The script can either be invoked from the project source tree, or the source
tree path can be provided as an argument e.g. `hookd.py /path/to/source`.

## Messages

The daemon communicates with bidirectional messages over STDIN and STDOUT.
Each message is terminated with a newline.
Newlines in values will be escaped as `\\n`.

``````
READY
    
    Signals that the daemon is ready to do work.
    Indicates that the daemon has finished running a hook.

EXPECT <name>

    Signals the input kind that the daemon expects.
    The name MUST be one of:
        - action
            Either "run" or "shutdown" instruction to the daemon
        - build_backend
            A PEP 517 import path for the build backend
        - hook_name
            A PEP 517 hook function name
        - wheel_directory
            A path
        - sdist_directory
            A path
        - config_settings
            A JSON payload
        - metadata_directory
            A path

DEBUG <message>

    A debugging message.

STDOUT <path>

    Sent before a hook is imported.
    The path to a file the hook's stdout will be directed to.
    The caller SHOULD delete this file when done with it.

STDERR <path>

    Sent before a hook is imported.
    The path to a file the hook's stderr will be directed to.
    The caller SHOULD delete this file when done with it.

OK <data>

    Sent when a hook completes successfully.
    The return value of the hook should follow.

ERROR <kind> <message>

    Sent when a hook fails.
    The error kind MUST be one of:
        - MissingBackendModule
        - MissingBackendAttribute
        - MalformedBackendName
        - BackendImportError
        - InvalidHookName
        - InvalidAction
        - UnsupportedHook
        - MalformedHookArgument
        - HookRuntimeError
    The message is a string describing the error. 

FATAL <kind> <message>

    Sent when the daemon crashes due to an unhandled error.

TRACEBACK <lines>

    MAY be sent after a FATAL or ERROR message.
    Contains the traceback for the error.

SHUTDOWN
    
    Signals that the daemon is exiting cleanly.
```
    
## Caveats

The following caveats apply when using hookd:

- Imports are cached for the duration of the daemon. Changes to the build backend packages will not be detected without restart.
- `BaseExceptions` raised during hook execution will be swallowed, unless from a SIGINT or SIGTERM.
