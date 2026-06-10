# Integration test organization

Each top-level directory in this folder is a Cargo integration test target. Test modules are grouped
by command area to limit the amount of code that Cargo needs to relink after a change, while large
suites use a dedicated target.

These groups are coarse compilation boundaries, not a strict product taxonomy. Add new test modules
to the closest existing target, and benchmark before introducing a target for each file.
