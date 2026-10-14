# Selected native debugger transcripts

These excerpts retain the debugger output, including unavailable values. Full raw traces and
artifacts are identified in manifest.json.

## Linux-X64: full optimized CLI

### live_values

```text
: (unsigned long) seed = <Could not evaluate DW_OP_GNU_entry_value.>

: (unsigned long) live_scalar = <variable not available>

0x000055a46d6f1db0: (unsigned long) constant = 42
0x00007ffeafc99b48: (capability_fixture::Record) record = {count:58, tag:23}
0x00007ffeafc99b58: ((u64, u64)) tuple = (17, 58)
0x00007ffeafc99b68: (core::option::Option<u64>) optional = {
0x00007ffeafc99b68:   (core::option::Option<u64>::core::option::Option<u64>$Inner) $variants$ = {$variant$0:{$discr$:1, value:{}}, $variant$1:{$discr$:1, value:{0:22}}}
}
0x00007ffeafc99b00: (capability_fixture::Choice) choice = {
0x00007ffeafc99b00:   (capability_fixture::Choice::capability_fixture::Choice$Inner) $variants$ = {$variant$0:{$discr$:1, value:{}}, $variant$1:{$discr$:1, value:{value:17, tag:91}}}
}
0x00007ffeafc99b18: (alloc::string::String) text = {
0x0000563bcd169e10:   (unsigned char) [0] = 'c'
0x0000563bcd169e11:   (unsigned char) [1] = 'a'
0x0000563bcd169e12:   (unsigned char) [2] = 'p'
0x0000563bcd169e13:   (unsigned char) [3] = 'a'
0x0000563bcd169e14:   (unsigned char) [4] = 'b'
0x0000563bcd169e15:   (unsigned char) [5] = 'i'
0x0000563bcd169e16:   (unsigned char) [6] = 'l'
0x0000563bcd169e17:   (unsigned char) [7] = 'i'
0x0000563bcd169e18:   (unsigned char) [8] = 't'
0x0000563bcd169e19:   (unsigned char) [9] = 'y'
0x0000563bcd169e1a:   (unsigned char) [10] = '-'
0x0000563bcd169e1b:   (unsigned char) [11] = 'p'
0x0000563bcd169e1c:   (unsigned char) [12] = 'r'
0x0000563bcd169e1d:   (unsigned char) [13] = 'o'
0x0000563bcd169e1e:   (unsigned char) [14] = 'b'
0x0000563bcd169e1f:   (unsigned char) [15] = 'e'
}
0x00007ffeafc99b30: (alloc::vec::Vec<unsigned long, alloc::alloc::Global>) items = size=3 {
0x0000563bcd169e30:   (unsigned long) [0] = 17
0x0000563bcd169e38:   (unsigned long) [1] = 58
0x0000563bcd169e40:   (unsigned long) [2] = 42
}
```

### optimized_values

```text
: (unsigned long) seed = <Could not evaluate DW_OP_GNU_entry_value.>

0x000055a46d6af110: (unsigned long) constant = 42
: (unsigned long) unused_value = <variable not available>

: (unsigned long) lower_byte = <variable not available>

rbx: (unsigned long) bucket = 1
: (unsigned long) consumed = <variable not available>

```

### shadow_inner

```text
: (unsigned long) seed = <Could not evaluate DW_OP_GNU_entry_value.>

r14: (unsigned long) shadowed = 27
rbx: (unsigned long) shadowed = 37
```

### split_record

```text
: (unsigned long) seed = <Could not evaluate DW_OP_GNU_entry_value.>

0x000055a46d7868b0: (capability_fixture::Record) partial = {count:19, tag:0}
```

### before_last_use

```text
rbx: (unsigned long) seed = 17
0x00007ffeafc99b98: (unsigned long) temporary = 85
```

### after_last_use

```text
rbx: (unsigned long) seed = 17
: (unsigned long) temporary = <variable not available>

```

## Linux-ARM64: full optimized CLI

### live_values

```text
: (unsigned long) seed = <Could not evaluate DW_OP_GNU_entry_value.>

: (unsigned long) live_scalar = <variable not available>

0x0000ab375f3ac4b0: (unsigned long) constant = 42
0x0000ffffd4f935a8: (capability_fixture::Record) record = {count:58, tag:23}
0x0000ffffd4f935b8: ((u64, u64)) tuple = (17, 58)
0x0000ffffd4f935c8: (core::option::Option<u64>) optional = {
0x0000ffffd4f935c8:   (core::option::Option<u64>::core::option::Option<u64>$Inner) $variants$ = {$variant$0:{$discr$:1, value:{}}, $variant$1:{$discr$:1, value:{0:22}}}
}
0x0000ffffd4f935d8: (capability_fixture::Choice) choice = {
0x0000ffffd4f935d8:   (capability_fixture::Choice::capability_fixture::Choice$Inner) $variants$ = {$variant$0:{$discr$:1, value:{}}, $variant$1:{$discr$:1, value:{value:17, tag:91}}}
}
0x0000ffffd4f935f0: (alloc::string::String) text = {
0x0000aba3a43e1e10:   (unsigned char) [0] = 'c'
0x0000aba3a43e1e11:   (unsigned char) [1] = 'a'
0x0000aba3a43e1e12:   (unsigned char) [2] = 'p'
0x0000aba3a43e1e13:   (unsigned char) [3] = 'a'
0x0000aba3a43e1e14:   (unsigned char) [4] = 'b'
0x0000aba3a43e1e15:   (unsigned char) [5] = 'i'
0x0000aba3a43e1e16:   (unsigned char) [6] = 'l'
0x0000aba3a43e1e17:   (unsigned char) [7] = 'i'
0x0000aba3a43e1e18:   (unsigned char) [8] = 't'
0x0000aba3a43e1e19:   (unsigned char) [9] = 'y'
0x0000aba3a43e1e1a:   (unsigned char) [10] = '-'
0x0000aba3a43e1e1b:   (unsigned char) [11] = 'p'
0x0000aba3a43e1e1c:   (unsigned char) [12] = 'r'
0x0000aba3a43e1e1d:   (unsigned char) [13] = 'o'
0x0000aba3a43e1e1e:   (unsigned char) [14] = 'b'
0x0000aba3a43e1e1f:   (unsigned char) [15] = 'e'
}
0x0000ffffd4f93608: (alloc::vec::Vec<unsigned long, alloc::alloc::Global>) items = size=3 {
0x0000aba3a43e1e30:   (unsigned long) [0] = 17
0x0000aba3a43e1e38:   (unsigned long) [1] = 58
0x0000aba3a43e1e40:   (unsigned long) [2] = 42
}
```

### optimized_values

```text
: (unsigned long) seed = <Could not evaluate DW_OP_GNU_entry_value.>

0x0000ab375f1d60a0: (unsigned long) constant = 42
: (unsigned long) unused_value = <variable not available>

: (unsigned long) lower_byte = <variable not available>

x19: (unsigned long) bucket = 1
: (unsigned long) consumed = <variable not available>

```

### shadow_inner

```text
: (unsigned long) seed = <Could not evaluate DW_OP_GNU_entry_value.>

x19: (unsigned long) shadowed = 27
x20: (unsigned long) shadowed = 37
```

### split_record

```text
: (unsigned long) seed = <Could not evaluate DW_OP_GNU_entry_value.>

0x0000ab375f1d4a50: (capability_fixture::Record) partial = {count:19, tag:0}
```

### before_last_use

```text
x19: (unsigned long) seed = 17
0x0000ffffd4f93668: (unsigned long) temporary = 85
```

### after_last_use

```text
x19: (unsigned long) seed = 17
: (unsigned long) temporary = <variable not available>

```

## macOS-ARM64: full optimized CLI

### live_values

```text
: (unsigned long) seed = <Could not evaluate DW_OP_entry_value.>

: (unsigned long) live_scalar = <variable not available>

0x00006000002d8e80: (unsigned long) constant = 42
0x000000016f807398: (capability_fixture::Record) record = {count:58, tag:23}
0x000000016f8073a8: ((u64, u64)) tuple = (17, 58)
0x000000016f8073b8: (core::option::Option<u64>) optional = {
0x000000016f8073b8:   (core::option::Option<u64>::core::option::Option<u64>$Inner) $variants$ = {$variant$0:{$discr$:1, value:{}}, $variant$1:{$discr$:1, value:{0:22}}}
}
0x000000016f8073c8: (capability_fixture::Choice) choice = {
0x000000016f8073c8:   (capability_fixture::Choice::capability_fixture::Choice$Inner) $variants$ = {$variant$0:{$discr$:1, value:{}}, $variant$1:{$discr$:1, value:{value:17, tag:91}}}
}
0x000000016f8073e0: (alloc::string::String) text = "capability-probe" {
0x00006000023b8010:   (unsigned char) [0] = 'c'
0x00006000023b8011:   (unsigned char) [1] = 'a'
0x00006000023b8012:   (unsigned char) [2] = 'p'
0x00006000023b8013:   (unsigned char) [3] = 'a'
0x00006000023b8014:   (unsigned char) [4] = 'b'
0x00006000023b8015:   (unsigned char) [5] = 'i'
0x00006000023b8016:   (unsigned char) [6] = 'l'
0x00006000023b8017:   (unsigned char) [7] = 'i'
0x00006000023b8018:   (unsigned char) [8] = 't'
0x00006000023b8019:   (unsigned char) [9] = 'y'
0x00006000023b801a:   (unsigned char) [10] = '-'
0x00006000023b801b:   (unsigned char) [11] = 'p'
0x00006000023b801c:   (unsigned char) [12] = 'r'
0x00006000023b801d:   (unsigned char) [13] = 'o'
0x00006000023b801e:   (unsigned char) [14] = 'b'
0x00006000023b801f:   (unsigned char) [15] = 'e'
}
0x000000016f8073f8: (alloc::vec::Vec<unsigned long, alloc::alloc::Global>) items = size=3 {
0x00006000021b40e0:   (unsigned long) [0] = 17
0x00006000021b40e8:   (unsigned long) [1] = 58
0x00006000021b40f0:   (unsigned long) [2] = 42
}
```

### optimized_values

```text
: (unsigned long) seed = <Could not evaluate DW_OP_entry_value.>

0x00006000002d9420: (unsigned long) constant = 42
: (unsigned long) unused_value = <variable not available>

: (unsigned long) lower_byte = <variable not available>

x19: (unsigned long) bucket = 1
: (unsigned long) consumed = <variable not available>

```

### shadow_inner

```text
: (unsigned long) seed = <Could not evaluate DW_OP_entry_value.>

x19: (unsigned long) shadowed = 27
x20: (unsigned long) shadowed = 37
```

### split_record

```text
: (unsigned long) seed = <Could not evaluate DW_OP_entry_value.>

0x0000600000284c50: (capability_fixture::Record) partial = {count:19, tag:0}
```

### before_last_use

```text
x19: (unsigned long) seed = 17
0x000000016f807450: (unsigned long) temporary = 85
```

### after_last_use

```text
x19: (unsigned long) seed = 17
: (unsigned long) temporary = <variable not available>

```

## Windows-X64: full optimized CDB

### live_values

```text
<unavailable>     unsigned int64 seed = <value unavailable>
<unavailable>     unsigned int64 live_scalar = <value unavailable>
<Addr: Internal implementation error> unsigned int64 constant = 0x2a
00000052`0956f4e8 struct capability_fixture::Record record = struct capability_fixture::Record
00000052`0956f4f8 struct tuple$<u64,u64> tuple = (0x11, 0x3a)
00000052`0956f508 union enum2$<core::option::Option<u64> > optional = Some
00000052`0956f4a0 union enum2$<capability_fixture::Choice> choice = Present
00000052`0956f4b8 struct alloc::string::String text = "capability-probe"
00000052`0956f4d0 struct alloc::vec::Vec<u64,alloc::alloc::Global> items = { len=0x3 }
<unavailable>     unsigned int64 selected = <value unavailable>
```

### shadow_inner

```text
@rsi              unsigned int64 shadowed = 0x25
@rdi              unsigned int64 shadowed = 0x1b
<unavailable>     unsigned int64 seed = <value unavailable>
```

### split_record

```text
                  struct capability_fixture::Record partial = struct capability_fixture::Record
<unavailable>     unsigned int64 seed = <value unavailable>
```

The aggregate display and typed field expression differ:

```text
struct capability_fixture::Record
   +0x000 count            : 0x13 (rsi)
   +0x008 tag              : Uint8B
```

Typed expressions: `{'partial_count': 19, 'partial_tag': 19}`. The source value for `tag` is 1717.

### before_last_use

```text
00000052`0956f538 unsigned int64 temporary = 0x55
@rsi              unsigned int64 seed = 0x11
```

### after_last_use

```text
<unavailable>     unsigned int64 temporary = <value unavailable>
@rsi              unsigned int64 seed = 0x11
```
