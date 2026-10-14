# Native debugger capabilities, October 14, 2026

Full debug enables inspection of Rust types, variables and object contents that line tables and
limited debug omit. It does not restore unoptimized program state. These native tests found both
unavailable variables and misleading displays of partially optimized objects.

The retained cohort has **20 successful fixture builds**: five variants on each of Linux x86-64,
Linux ARM64, macOS ARM64 and Windows x86-64. Final observations completed for all 20 variants using
LLDB on Unix and CDB on Windows, with successful unoptimized full-debug positive controls. Earlier
harness failures and the Windows LLDB calibration failure remain recorded below. These are small
Rust+C fixtures, not an inspection of every variable in uv or a core-dump compatibility test.

## Observed capability matrix

All three optimized symbol modes use Rust 1.99.0 / LLVM 23.1.1, optimization level 3, final fat LTO,
one CGU and independently trained PGO profiles. Windows uses static CRT. See the
[methods](../../CAPABILITIES.md) for controls, source and reproduction instructions.

| Operation                                                              | Line tables only        | Limited               | Full                                                            |
| ---------------------------------------------------------------------- | ----------------------- | --------------------- | --------------------------------------------------------------- |
| File/line locations in native fixture stacks, all four targets         | Yes                     | Yes                   | Yes                                                             |
| File/line breakpoint and six source step-over operations, Unix LLDB    | Yes                     | Yes                   | Yes                                                             |
| Inline leaf and middle frames, Unix LLDB                               | Both, unqualified names | Both, qualified names | Both, qualified names                                           |
| Inline leaf and middle frames at the optimized checkpoint, Windows CDB | Not reconstructed       | Not reconstructed     | Not reconstructed                                               |
| Rust `Record` type and field layout                                    | Absent                  | Absent                | Present                                                         |
| Rust locals, arguments and struct contents                             | Omitted                 | Omitted               | Available where the compiler/debugger retains a usable location |
| Known live `Record { count: 58, tag: 23 }`, all four targets           | Unavailable             | Unavailable           | Both fields correct                                             |
| Inner/outer shadowed values, all four optimized targets                | Unavailable             | Unavailable           | 37 inside, 27 outside                                           |
| Local before versus after its last use, all four targets               | Unavailable             | Unavailable           | 85 before; unavailable afterward                                |
| Tail-call forwarding frame in optimized trace, all four targets        | Absent                  | Absent                | Absent                                                          |

Windows CDB source stepping was not exercised. Its stack file/line observations do not establish
stepping quality. In the unoptimized Windows control, CDB reconstructed both forced inline frames;
only the optimized checkpoint lost them. Other inline frames from the standard library were visible,
so this does not establish a general absence of Windows inline information.

The stripped no-debug controls had no fixture source locations or Rust variables. Removing the
companion also removed fixture source information in the Unix controls. The independent Windows CDB
missing-PDB control lost both source locations and named locals. A deliberately exported checkpoint
lets the harness stop in these controls without private symbols.

## Optimization edge cases

| Full-debug probe                 | Observed result                                                                                                                                                                                                                                                     |
| -------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Named scalar versus stored field | `live_scalar` is unavailable, but the same value is readable as `record.count = 58` on all four targets.                                                                                                                                                            |
| Constant and intermediate        | `constant = 42` and `bucket = 1` remain readable. The unused arithmetic result, bit-mask intermediate and already-consumed value are unavailable in the optimized frame.                                                                                            |
| Function argument                | Unix LLDB reports a `DW_OP_entry_value` / `DW_OP_GNU_entry_value` evaluation error for `seed` in several optimized frames. CDB also reports an unavailable argument in the corresponding live-values frame. This is not proof that the runtime value never existed. |
| Last use                         | `temporary = 85` is readable before its last use and unavailable afterward. It remains 85 at both stops in the unoptimized controls.                                                                                                                                |
| Shadowing                        | Lexical lookup finds the inner value 37 and outer value 27. Enumeration can contain both bindings; selecting the first name match is incorrect. CDB lists both in the unoptimized inner scope, while its unqualified typed expression is ambiguous there.           |
| Inlined local                    | Full debug reads `inline_value = 34` in the reconstructed Unix inline frame. Windows CDB did not reconstruct that optimized frame or recover that local at this checkpoint.                                                                                         |
| Tail call                        | `tail_root` appears with `tail_leaf` in the unoptimized controls but disappears from all optimized traces. Full debug did not reconstruct an artificial caller.                                                                                                     |

A particularly important partial-object result is:

```rust
let partial = Record {
    count: seed.wrapping_add(2),   // 19 for the inspected seed
    tag: seed.wrapping_mul(101),  // 1717 in source semantics
};
capability_checkpoint(6);
black_box(partial.count)
```

Only `count` is consumed. At the optimized stop, LLDB's ordinary `frame variable -T -L` command
printed **`{count:19, tag:0}` on all three Unix targets**. The relevant DWARF location describes
only the first eight-byte piece of this sixteen-byte object at that stop. The displayed zero must
not be treated as the source value of `tag`. The retained DWARF extracts show the register/piece
expressions; this report does not assign the entire behavior to one compiler or debugger component.

Windows exposes a related discrepancy: CDB's `?? partial` shows `count = 19` and gives `tag` a type
without a value, but the typed expression `@@c++(partial.tag)` returns **19**, not 1717. The two
commands must not be collapsed into a claim that the field was recovered correctly. All unoptimized
controls recover `count = 19, tag = 1717`.

These are concrete reasons to treat optimized debugger output as partial evidence. Enabling full
debug provides useful state that the lower levels intentionally omit; it cannot guarantee that every
source variable or field is accurately recoverable at every instruction.

## Rust values and native C

With full debug, all three Unix targets expose the tuple `(17, 58)`, vector elements `[17, 58, 42]`
and string bytes for `capability-probe`. macOS LLDB also renders the string as a quoted summary;
Linux LLDB renders its character children without that summary. `Option` and the custom enum expose
discriminants and nested payloads, but the display uses internal `$variants$` layouts rather than a
clean Rust expression. Full type information is necessary here, but formatter quality still matters.

Windows CDB exposes the Rust struct and container layouts, enum/option discriminants and vector
length 3. The recorded default aggregate display does not provide a polished Rust string/vector or
enum presentation. Its `Choice` type is encoded as `enum2$<capability_fixture::Choice>`; a lookup
using the Unix spelling alone does not prove the type is absent. The raw aggregate output is
retained. No claim is made that every container element was inspected in CDB.

Native C information is independent of the Rust debug level. Linux retains the C argument 17, local
52 and structure fields 52/26 with all three symbol modes. macOS retains the C argument, but the
optimized C scalar has no location and the structure displays 0/0 rather than the source values
52/26. This is another misleading optimized-object display. Windows retains the C argument, but its
optimized C local and structure were not recoverable in these CDB observations. Reduced Rust debug
does not imply reduced native C flags or identical native compiler decisions.

## Tools, controls and provenance

| Target         | Final debugger                       | Observation job                                                                          |
| -------------- | ------------------------------------ | ---------------------------------------------------------------------------------------- |
| Linux x86-64   | LLDB 18.1.3                          | [Unix replay](https://github.com/astral-sh/uv/actions/runs/38604711140/job/115992535117) |
| Linux ARM64    | LLDB 18.1.3                          | [Unix replay](https://github.com/astral-sh/uv/actions/runs/38604711140/job/115992535096) |
| macOS ARM64    | Apple LLDB 1600.0.36.3 / Swift 6.0   | [Unix replay](https://github.com/astral-sh/uv/actions/runs/38604711140/job/115992535031) |
| Windows x86-64 | Windows Debugger/CDB 10.0.26100.4202 | [CDB replay](https://github.com/astral-sh/uv/actions/runs/38605179316/job/115994120165)  |

All final observers use the binaries from
[build run 38601818261](https://github.com/astral-sh/uv/actions/runs/38601818261), compiled at
`737bf01b670a84a6ada1afe33b2f0421efe2e7ca`. Replays verify the original report, executable and
companion hashes and record separate observer commits. Collection also independently checked both
instrumented and final compiler commands and four distinct PGO profile hashes per target. All native
executions produced checksum 536. Downloaded Windows/Linux executables were not run on the
collecting Mac.

The following failures and observer corrections are preserved rather than counted as capability
failures or silently discarded:

- [Initial run 38601046009](https://github.com/astral-sh/uv/actions/runs/38601046009): Windows hit a
  Python 3.10-incompatible hashing API before building; Unix built all variants but the observer
  requested an obsolete Rust LLDB formatter command file before target launch.
- [Build run 38601818261](https://github.com/astral-sh/uv/actions/runs/38601818261): all 20 builds
  and checksums succeeded. Unix debugger controls passed. Windows LLDB failed the unoptimized
  positive control, so its variable/stack observations were not used to infer PDB capability.
- [Unix replay 38604711140](https://github.com/astral-sh/uv/actions/runs/38604711140): corrected
  qualified/unqualified inline-name matching, used lexical lookup for shadowing, and recorded
  ordinary CLI output to cross-check structured debugger values. All three native targets passed.
- [CDB replay 38604717552](https://github.com/astral-sh/uv/actions/runs/38604717552): a command-file
  path escaped inside the breakpoint string prevented observation; the strict symbol option also
  prevented the export-only control from resolving. This was an observer failure.
- [CDB replay 38605179316](https://github.com/astral-sh/uv/actions/runs/38605179316): corrected
  observer commands, reused the same binaries/PDBs, and passed the positive and missing-PDB
  controls.

The local host disallowed live LLDB attachment, so live observations came from native CI. This is
separate from symbol support. Push-triggered Rust+C packaging fixtures and harness tests are
auxiliary checks, not additional observations in this capability comparison.

[Measurements](measurements.json) retain build hashes/flags, structured values, source and inline
observations, CLI output, negative controls and debugger versions.
[Selected transcripts](debugger-excerpts.md) show the critical value displays.
[Manifest](manifest.json) records artifact IDs/digests, exact source/observer commits,
runner/job/attempt provenance and earlier failed evidence. Full archives, job logs, PGO profiles and
debugger traces remain under `target/debug-symbols-capabilities-2026-10-14/`, separated by run and
platform. The adjacent partial DWARF extracts preserve the location expressions used to investigate
the misleading struct display.

The fixtures deliberately use address-escaped objects as positive controls, forced inlining and an
exported checkpoint. Those choices are useful for testing capabilities but do not estimate how
frequently a particular uv variable will be recoverable. No optimization settings were weakened to
make the optimized observations pass. No debugger/compiler bug report or production symbol-policy
change is implied by this experiment.
