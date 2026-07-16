<!-- Generated with `cargo dev render-scenario` -->

# remove-prune-graph

Dependency graphs for subset-removal integration tests.

```text
remove-prune-graph
├── environment
│   └── python3.12
├── root
├── b
│   ├── b-1.0.0
│   └── b-1.0.0[foo]
│       └── requires candidate
│           └── satisfied by candidate-1.0.0
├── base-external
│   └── base-external-1.0.0
│       └── requires base-extra-root[foo]
│           ├── satisfied by base-extra-root-1.0.0
│           └── satisfied by base-extra-root-1.0.0[foo]
├── base-extra-root
│   ├── base-extra-root-1.0.0
│   │   └── requires base-leaf ; extra != 'foo'
│   │       └── satisfied by base-leaf-1.0.0
│   └── base-extra-root-1.0.0[foo]
├── base-leaf
│   └── base-leaf-1.0.0
├── base-removed
│   └── base-removed-1.0.0
│       └── requires base-leaf
│           └── satisfied by base-leaf-1.0.0
├── bridge
│   ├── bridge-1.0.0
│   └── bridge-1.0.0[foo]
│       └── requires candidate
│           └── satisfied by candidate-1.0.0
├── build-only
│   └── build-only-1.0.0
├── candidate
│   └── candidate-1.0.0
├── cycle-a
│   └── cycle-a-1.0.0
│       └── requires cycle-b
│           └── satisfied by cycle-b-1.0.0
├── cycle-b
│   └── cycle-b-1.0.0
│       └── requires cycle-a
│           └── satisfied by cycle-a-1.0.0
├── cycle-external
│   └── cycle-external-1.0.0
│       └── requires cycle-b
│           └── satisfied by cycle-b-1.0.0
├── cycle-root
│   └── cycle-root-1.0.0
│       └── requires cycle-a
│           └── satisfied by cycle-a-1.0.0
├── edge-root
│   └── edge-root-1.0.0
│       └── requires bridge[foo]
│           ├── satisfied by bridge-1.0.0
│           └── satisfied by bridge-1.0.0[foo]
├── external
│   └── external-1.0.0
│       └── requires candidate
│           └── satisfied by candidate-1.0.0
├── marker-candidate
│   └── marker-candidate-1.0.0
│       └── requires python>=3.11
├── marker-removed
│   └── marker-removed-1.0.0
│       └── requires candidate ; python_full_version < '3.12'
│           └── satisfied by candidate-1.0.0
├── marker-root
│   └── marker-root-1.0.0
│       ├── requires marker-candidate
│       │   └── satisfied by marker-candidate-1.0.0
│       └── requires python>=3.11
├── orphan
│   └── orphan-1.0.0
│       └── requires orphan-leaf
│           └── satisfied by orphan-leaf-1.0.0
├── orphan-leaf
│   └── orphan-leaf-1.0.0
├── other
│   └── other-1.0.0
├── removed
│   └── removed-1.0.0
│       ├── requires candidate
│       │   └── satisfied by candidate-1.0.0
│       └── requires orphan
│           └── satisfied by orphan-1.0.0
└── root-external
    └── root-external-1.0.0
        └── requires removed
            └── satisfied by removed-1.0.0
```
