//! Compile Python source to CPython bytecode without starting a Python interpreter.
//!
//! This crate currently targets CPython 3.14.5 exclusively. The bytecode, code object,
//! marshal, and `.pyc` formats are all implementation details that can change between
//! Python releases, so future targets should be implemented as separate backends.

mod assembler;
mod compiler;
mod marshal;

use std::error::Error;
use std::fmt;

use compiler::{CodeObject, Compiler};

/// The exact CPython implementation targeted by this compiler backend.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CpythonTarget {
    /// The value of `sys.implementation.name`.
    pub implementation: &'static str,
    /// The value of `sys.version_info[:3]`.
    pub version: (u8, u8, u8),
    /// The value of `importlib.util.MAGIC_NUMBER`.
    pub magic_number: [u8; 4],
}

impl CpythonTarget {
    /// Return the dotted form of [`Self::version`].
    pub fn version_string(self) -> String {
        let (major, minor, micro) = self.version;
        format!("{major}.{minor}.{micro}")
    }

    /// Return the lowercase hexadecimal form of [`Self::magic_number`].
    pub fn magic_hex(self) -> String {
        const HEX: &[u8; 16] = b"0123456789abcdef";

        let mut output = String::with_capacity(self.magic_number.len() * 2);
        for byte in self.magic_number {
            output.push(char::from(HEX[usize::from(byte >> 4)]));
            output.push(char::from(HEX[usize::from(byte & 0x0f)]));
        }
        output
    }
}

/// The sole CPython target currently implemented by this crate.
pub const CPYTHON_TARGET: CpythonTarget = CpythonTarget {
    implementation: "cpython",
    version: (3, 14, 5),
    magic_number: [0x2b, 0x0e, 0x0d, 0x0a],
};

/// A compiled CPython 3.14.5 module code object.
#[derive(Clone, Debug)]
pub struct CompiledModule {
    code: CodeObject,
}

impl CompiledModule {
    /// Return the module's raw CPython instruction stream.
    pub fn bytecode(&self) -> &[u8] {
        &self.code.bytecode
    }

    /// Marshal the module code object using CPython 3.14.5's format.
    pub fn marshal(&self) -> Vec<u8> {
        marshal::encode_code(&self.code)
    }

    /// Build a timestamp-invalidated CPython 3.14.5 `.pyc` file.
    ///
    /// `source_mtime` is the source file's modification time in whole seconds since the
    /// Unix epoch. `source_size` is the source file's byte length, truncated to 32 bits as
    /// required by CPython's cache format.
    pub fn to_timestamp_pyc(&self, source_mtime: u32, source_size: u32) -> Vec<u8> {
        let marshalled = self.marshal();
        let mut output = Vec::with_capacity(16 + marshalled.len());
        output.extend_from_slice(&CPYTHON_TARGET.magic_number);
        output.extend_from_slice(&0_u32.to_le_bytes());
        output.extend_from_slice(&source_mtime.to_le_bytes());
        output.extend_from_slice(&source_size.to_le_bytes());
        output.extend_from_slice(&marshalled);
        output
    }
}

/// A source parsing or bytecode generation error.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CompileError {
    /// Ruff rejected the Python source.
    Parse(String),
    /// The source uses valid Python syntax that this backend does not support yet.
    Unsupported(String),
    /// The compiler violated one of its own invariants.
    Internal(String),
}

impl fmt::Display for CompileError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Parse(message) => write!(formatter, "failed to parse Python source: {message}"),
            Self::Unsupported(message) => formatter.write_str(message),
            Self::Internal(message) => write!(formatter, "internal compiler error: {message}"),
        }
    }
}

impl Error for CompileError {}

/// Compile a Python module for CPython 3.14.5.
pub fn compile(source: &str, filename: &str) -> Result<CompiledModule, CompileError> {
    let normalized;
    let source = if source.contains('\r') {
        normalized = source.replace("\r\n", "\n").replace('\r', "\n");
        normalized.as_str()
    } else {
        source
    };
    let source = source.strip_prefix('\u{feff}').unwrap_or(source);
    let options = ruff_python_parser::ParseOptions::from(ruff_python_parser::Mode::Module)
        .with_target_version(ruff_python_ast::PythonVersion::PY314);
    let parsed = ruff_python_parser::parse(source, options)
        .map_err(|error| CompileError::Parse(error.to_string()))?
        .try_into_module()
        .ok_or_else(|| CompileError::Internal("parser did not return a module".to_string()))?;
    let code = Compiler::module(filename, source).compile_module(parsed.suite())?;
    Ok(CompiledModule { code })
}

/// Compile a Python module directly to a timestamp-invalidated CPython 3.14.5 `.pyc` file.
pub fn compile_to_pyc(
    source: &str,
    filename: &str,
    source_mtime: u32,
) -> Result<Vec<u8>, CompileError> {
    let source_size = u32::try_from(source.len()).map_err(|_| {
        CompileError::Unsupported("Python source exceeds the 4 GiB `.pyc` limit".to_string())
    })?;
    Ok(compile(source, filename)?.to_timestamp_pyc(source_mtime, source_size))
}

#[cfg(test)]
mod tests {
    use std::fmt::Write as _;
    use std::io::Write as _;
    use std::process::{Command, Stdio};

    use tempfile::tempdir;

    use super::{CPYTHON_TARGET, compile, compile_to_pyc};

    const CPYTHON_MARSHAL: &str = "import marshal, sys; code = compile(sys.stdin.read(), sys.argv[1], 'exec', dont_inherit=True, optimize=0); sys.stdout.buffer.write(marshal.dumps(code))";

    fn python_3145() -> Option<String> {
        let configured = std::env::var("UV_PYTHON_BYTECODE_TEST_PYTHON").ok();
        let executable = configured.as_deref().unwrap_or("python3.14");
        let version = CPYTHON_TARGET.version_string();
        let magic = CPYTHON_TARGET.magic_hex();
        let output = Command::new(executable)
            .args([
                "-c",
                "import importlib.util, sys; expected = tuple(map(int, sys.argv[2].split('.'))); actual = sys.version_info[:3]; magic = importlib.util.MAGIC_NUMBER.hex(); ok = sys.implementation.name == sys.argv[1] and actual == expected and magic == sys.argv[3]; print(f'{sys.implementation.name} {'.'.join(map(str, actual))} magic {magic}', file=sys.stderr); raise SystemExit(not ok)",
                CPYTHON_TARGET.implementation,
                version.as_str(),
                magic.as_str(),
            ])
            .output()
            .ok()?;
        if output.status.success() {
            return Some(executable.to_string());
        }
        if configured.is_some() {
            panic!(
                "UV_PYTHON_BYTECODE_TEST_PYTHON must name CPython {} with magic {}: {}",
                version,
                magic,
                String::from_utf8_lossy(&output.stderr).trim()
            );
        }
        None
    }

    fn assert_matches_cpython_marshal(source: &str, filename: &str) {
        let Some(python) = python_3145() else {
            return;
        };
        let expected = Command::new(python)
            .args(["-c", CPYTHON_MARSHAL, filename])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .and_then(|mut child| {
                child.stdin.take().unwrap().write_all(source.as_bytes())?;
                child.wait_with_output()
            })
            .unwrap();
        assert!(expected.status.success());
        assert_eq!(
            compile(source, filename).unwrap().marshal(),
            expected.stdout
        );
    }

    fn assert_executes(source: &str, expected: &str) {
        let Some(python) = python_3145() else {
            return;
        };
        let temporary = tempdir().unwrap();
        let pyc_path = temporary.path().join("program.pyc");
        let pyc = compile_to_pyc(source, "program.py", 0).unwrap();
        fs_err::write(&pyc_path, pyc).unwrap();

        let output = Command::new(python).arg(&pyc_path).output().unwrap();
        assert!(
            output.status.success(),
            "generated bytecode failed:\n{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(String::from_utf8(output.stdout).unwrap(), expected);
    }

    #[test]
    fn emits_a_python_3145_pyc_header() {
        let pyc = compile_to_pyc("answer = 42\n", "answer.py", 123).unwrap();
        assert_eq!(&pyc[..4], &CPYTHON_TARGET.magic_number);
        assert_eq!(&pyc[4..8], &0_u32.to_le_bytes());
        assert_eq!(&pyc[8..12], &123_u32.to_le_bytes());
        assert_eq!(&pyc[12..16], &12_u32.to_le_bytes());
    }

    #[test]
    fn normalizes_bom_and_newlines_before_compiling() {
        let source =
            "\u{feff}def greet(name):\r\n    message = f\"héllo, {name}\"\r    return message\r\n";
        let normalized = "def greet(name):\n    message = f\"héllo, {name}\"\n    return message\n";

        assert_eq!(
            compile(source, "greet.py").unwrap().marshal(),
            compile(normalized, "greet.py").unwrap().marshal()
        );
    }

    #[test]
    fn matches_cpython_marshal_for_an_empty_module() {
        assert_matches_cpython_marshal("", "empty.py");
    }

    #[test]
    fn matches_cpython_marshal_for_a_module_containing_only_global_statements() {
        let source = "global x\nglobal x, y, z\n";
        assert_matches_cpython_marshal(source, "global.py");
    }

    #[test]
    fn matches_cpython_marshal_when_a_match_case_ends_in_a_constant_expression() {
        let source = "match foo:\n    case foo_bar: ...\nmatch foo:\n    case _: ...\nmatch 1:\n    case _ if (True): ...\nmatch (1, 2):\n    case _: ...\nmatch subject:\n    case [a, b]: ...\n    case (a, b): ...\nmatch value:\n    case 1:\n        h(x)\n    case _:\n        ...\ndef terminal_try_star():\n    try:\n        assigned = 1\n    except* ValueError:\n        assigned = 2\ntry: ...\nexcept* ValueError: ...\nraise terminal\nmatch subject:\n    case unreachable_capture:\n        pass\n[temporary for temporary in values]\n\ndef match_assignment(value):\n    match value:\n        case True:\n            assigned = 1\n        case False:\n            assigned = 2\n    return assigned\n\ndef subject_before_capture(value):\n    match match:\n        case case:\n            pass\n    match = value\n    return match, case\n\ndef terminal_default_if(value):\n    match value:\n        case 1:\n            pass\n        case _:\n            if value != 2:\n                pass\n            elif value == 2:\n                pass\n\ndef terminal_nondefault_if(value):\n    match value:\n        case 1:\n            if value != 2:\n                pass\n            elif value == 2:\n                pass\n        case _:\n            pass\n";
        assert_matches_cpython_marshal(source, "match.py");
    }

    #[test]
    fn matches_cpython_marshal_for_a_match_case_ending_in_if() {
        let source = r#"match shopper:
    case "Jane":
        consume(shopper)
        if condition:
            consume(condition)
    case _:
        fallback(shopper)
"#;
        assert_matches_cpython_marshal(source, "match_tail_if.py");
    }

    #[test]
    fn matches_cpython_marshal_for_class_footer_control_flow() {
        let source = "class WithFooter:\n    with manager:\n        def method(self): ...\n\nclass WildcardFooter:\n    match value:\n        case int(): ...\n        case _:\n            def method(self): ...\n\nclass FoldedBranchFooter:\n    if ...:\n        ...\n    else:\n        with manager:\n            for value in values:\n                def method(self): ...\n";
        assert_matches_cpython_marshal(source, "class_footer.py");
    }

    #[test]
    fn matches_cpython_marshal_for_long_or_pattern_case_boundaries() {
        let source = "match x:\n    case \"abcd\" | \"abcd\" | \"abcd\" :\n        pass\n    case \"abcd\" | \"abcd\" | \"abcd\" | \"abcd\" | \"abcd\" | \"abcd\" | \"abcd\" | \"abcd\" | \"abcd\" | \"abcd\" | \"abcd\" | \"abcd\" | \"abcd\" | \"abcd\" | \"abcd\":\n        pass\n    case xxxxxxxxxxxxxxxxxxxxxxx:\n        pass\n";
        assert_matches_cpython_marshal(source, "pattern_matching_long.py");
    }

    #[test]
    fn matches_cpython_marshal_for_an_unreachable_generic_type_alias() {
        let source = "raise Exception\nassert False\ntype X[T] = T\n";
        assert_matches_cpython_marshal(source, "type_alias.py");
    }

    #[test]
    fn matches_cpython_marshal_for_generic_function_defaults() {
        let source = "def positional[T](value: T = T):\n    pass\n\ndef keyword[T](*, value: T = T):\n    pass\n\ndef both[T](value: T = T, *, other: T = T):\n    pass\n";
        assert_matches_cpython_marshal(source, "generic_defaults.py");
    }

    #[test]
    fn matches_cpython_marshal_for_nested_generic_class_scopes() {
        let source = "class Outer[T]:\n    class Inner:\n        function = lambda: None\n        values = (value for value in ())\n\n        def method(self, value: T) -> T:\n            return value\n\n        def generic[U](self, value: U) -> U:\n            return value\n";
        assert_matches_cpython_marshal(source, "nested_generic.py");
    }

    #[test]
    fn matches_cpython_marshal_for_generic_wrapper_closures() {
        let source = r#"def outer():
    value = int

    class Nested[T]:
        item: value

    return Nested


class Generic[T]:
    def method[U](self):
        return T, U
"#;
        assert_matches_cpython_marshal(source, "generic_closures.py");
    }

    #[test]
    fn matches_cpython_marshal_for_annotation_scope_closures() {
        let source = r#"from __future__ import annotations

def outer():
    from . import dependency

    class Generic[T: dependency.Type](dependency.Base):
        pass

    def function[T = dependency.Type](value: T):
        pass

    type Alias[T: dependency.Type] = tuple[dependency.Type, T]
    type Plain = dependency.Type


class Container:
    Local = object

    class Nested[T: Local](Local):
        pass


class CapturesBase[T](factory(lambda: T), metaclass=factory(lambda: T)):
    pass
"#;
        assert_matches_cpython_marshal(source, "annotation_scopes.py");
    }

    #[test]
    fn matches_cpython_marshal_for_class_type_aliases() {
        let source = "from __future__ import annotations\n\nclass Container:\n    Local = int\n    type Alias = Local\n    type Generic[T] = tuple[Local, T]\n    type Factory[T] = lambda: (Local, T)\n";
        assert_matches_cpython_marshal(source, "class_alias.py");
    }

    #[test]
    fn matches_cpython_marshal_for_string_slice_constants() {
        let source = "subject[\"fine\":\"fine\"]\nsubject[\"x\":\"x\"]\n";
        assert_matches_cpython_marshal(source, "slice.py");
    }

    #[test]
    fn matches_cpython_marshal_for_a_try_at_the_end_of_a_loop() {
        let source = "async def show_status():\n    while True:\n        try:\n            if report_host:\n                data = value\n        except Exception as error:\n            pass\n";
        assert_matches_cpython_marshal(source, "loop_try.py");
    }

    #[test]
    fn matches_cpython_marshal_for_try_exits_across_exception_boundaries() {
        let source = r#"try:
    1 / 0
except ZeroDivisionError:
    pass

try:
    raise ValueError
except ValueError as error:
    print(error)

try:
    pass
except Exception:
    pass
finally:
    try:
        pass
    except BaseException as error:
        raise error

try:
    pass
except Exception as error:
    try:
        raise error
    except BaseException:
        pass

try:
    try:
        pass
    except BaseException as error:
        raise error
except Exception:
    pass
"#;
        assert_matches_cpython_marshal(source, "try_exit.py");
    }

    #[test]
    fn matches_cpython_marshal_for_small_with_exits() {
        let source = "def normal():\n    with manager:\n        value = 0\n    value = 1\n\ndef terminal():\n    with manager:\n        value = 0\n        return 1\n    value = 1\n\ndef nested_suppression():\n    with first, \\\n        second:\n        raise Error\n    return result\n";
        assert_matches_cpython_marshal(source, "with_exit.py");
    }

    #[test]
    fn matches_cpython_marshal_for_async_with_returns() {
        let source = "async def value(manager):\n    async with manager as result:\n        return result.value\n\nasync def constant(manager):\n    async with manager:\n        return 1\n\nasync def nested(outer, inner):\n    async with outer:\n        with inner:\n            return value\n";
        assert_matches_cpython_marshal(source, "async_with_return.py");
    }

    #[test]
    fn matches_cpython_marshal_for_loop_control_unwinding() {
        let source = "def sync_context(manager, items):\n    for item in items:\n        with manager:\n            if item > 0:\n                continue\n            break\n\ndef exception_handler(items):\n    for item in items:\n        try:\n            consume(item)\n        except Exception:\n            handle(item)\n            continue\n        use(item)\n\ntry:\n    for item in items:\n        consume(item)\n        break\nexcept TypeError:\n    handle()\nafter()\n";
        assert_matches_cpython_marshal(source, "loop_control_unwind.py");
    }

    #[test]
    fn matches_cpython_marshal_for_async_comprehension_cleanup_exits() {
        let source = "async def discarded():\n    [item async for item in source]\n\nasync def returned():\n    return [item async for item in source]\n";
        assert_matches_cpython_marshal(source, "async_comprehension.py");
    }

    #[test]
    fn matches_cpython_marshal_for_nested_async_comprehension_cleanup_order() {
        let source =
            "async def test(): return [[x async for x in elements(n)] async for n in range(3)]\n";
        assert_matches_cpython_marshal(source, "nested_async_comprehension.py");
    }

    #[test]
    fn matches_cpython_marshal_for_multiline_constant_match_guards() {
        let source = "match 1:\n    case _ if (True):\n        pass\n\nmatch 1:\n    case _ if (\n        True\n    ):\n        pass\n";
        assert_matches_cpython_marshal(source, "match_guard.py");
    }

    #[test]
    fn matches_cpython_marshal_for_pass_only_try_else_finally() {
        let source = "try:\n    foo()\nexcept Exception:\n    pass\nelse:\n    pass\nfinally:\n    pass\n\ntry:\n    pass\nexcept Exception:\n    pass\nraise (\n    Exception\n)\n";
        assert_matches_cpython_marshal(source, "try.py");
    }

    #[test]
    fn matches_cpython_marshal_for_noop_finally_bodies() {
        let source = r#"try:
    body()
except Error:
    recover()
finally:
    ...

try:
    body()
except Error:
    recover()
finally:
    pass
    pass

try:
    try:
        body()
    finally:
        pass
except Error:
    recover()
"#;
        assert_matches_cpython_marshal(source, "try_finally_noops.py");
    }

    #[test]
    fn matches_cpython_marshal_for_multiple_exception_group_handlers() {
        let source = "try:\n    body()\nexcept* ValueError:\n    first()\nexcept* TypeError:\n    second()\n";
        assert_matches_cpython_marshal(source, "try_star.py");
    }

    #[test]
    fn matches_cpython_marshal_for_a_return_from_an_exception_handler() {
        let source = "def f():\n    value = source()\n    try:\n        raise ValueError\n    except ValueError:\n        return value\n\ndef nested():\n    try:\n        body()\n    except ValueError as error:\n        try:\n            pass\n        except TypeError:\n            raise error\n\ndef nested_message():\n    try:\n        body()\n    except ValueError as error:\n        try:\n            finish()\n        except TypeError:\n            log(f\"error: {error}\")\n";
        assert_matches_cpython_marshal(source, "handler_return.py");
    }

    #[test]
    fn matches_cpython_marshal_for_folded_expressions_in_protected_regions() {
        let source = "def try_except():\n    try:\n        'try'\n        body()\n    except:\n        'except'\n        recover()\n\ndef except_only():\n    try:\n        body()\n    except:\n        'except'\n\ndef nested_handlers():\n    try:\n        try:\n            body()\n        except ValueError:\n            recover()\n        except TypeError:\n            recover()\n    finally:\n        cleanup()\n\ndef with_body(manager):\n    with manager:\n        'with'\n        body()\n\ndef with_only(manager):\n    with manager:\n        'with'\n\nasync def protected_condition():\n    await source\n    if test:\n        await other\n\ntry:\n    pass\nexcept (ValueError if True else TypeError):\n    pass\n\ntry:\n    pass\nexcept* (ValueError if True else TypeError):\n    pass\n\ndef protected_conditions(manager, first, second):\n    with manager:\n        if first:\n            body()\n        if second:\n            body()\n\ndef generator_try():\n    yield 1\n    try:\n        pass\n    except Exception:\n        pass\n\nif condition:\n    try:\n        with manager:\n            body()\n    except Error:\n        pass\n";
        assert_matches_cpython_marshal(source, "protected_nop.py");
    }

    #[test]
    fn matches_cpython_marshal_for_folded_large_integer_bitwise_operations() {
        let source = "bit_and = 99999999999999999999999 & 0o200000\nbit_or = 99999999999999999999999 | 0o777\nbit_xor = 18446744073709551616 ^ 18446744073709551616\nsum = 9999999999999999999999999999999999999999 + 9999999999999999999999999999999999999999\n";
        assert_matches_cpython_marshal(source, "large_bitwise.py");
    }

    #[test]
    fn matches_cpython_marshal_for_folded_branches_in_a_with_statement() {
        let source = "from contextlib import suppress\n\ndef f():\n    with suppress(Exception):\n        if 1:\n            pass\n        elif 1:\n            pass\n        elif 1:\n            pass\n        try:\n            pass\n        except Exception:\n            pass\n        finally:\n            pass\n        if 2:\n            pass\n        while True:\n            pass\n";
        assert_matches_cpython_marshal(source, "protected_branches.py");
    }

    #[test]
    fn matches_cpython_marshal_for_coroutine_finally_condition() {
        let source = "async def f():\n    try:\n        await source\n    finally:\n        if test:\n            await other\n";
        assert_matches_cpython_marshal(source, "finally_condition.py");
    }

    #[test]
    fn matches_cpython_marshal_for_optimized_boolean_operands() {
        let source = "if a and f() and False and g():\n    pass\na or \"\" or True\na or () or True\na and \"value\" and False\na or (b or c) or d\n0 if a or [1] or True or [2] else 1\nif (\n    f()\n    is None\n):\n    pass\nif g() is not None:\n    pass\nx\n\ndef terminal_conditional_expression(wait):\n    call() if wait else None\n\nif first or low <= value < high:\n    pass\n\nmultiline = [\n    (\n        \"value\"\n        and condition\n        and result\n    )\n]\n";
        assert_matches_cpython_marshal(source, "bool.py");
    }

    #[test]
    fn matches_cpython_3145_boolean_exception_ranges() {
        let source = "async def choose(value):\n    return value or 'empty'\n\ntry:\n    body()\nexcept FirstError or SecondError as error:\n    recover(error)\n\ntry:\n    body()\nexcept FirstError and SecondError as error:\n    recover(error)\n";
        assert_matches_cpython_marshal(source, "boolean_exception_ranges.py");
    }

    #[test]
    fn matches_cpython_marshal_for_backward_boolean_assert_jumps() {
        let source = "def f(items):\n    for item in items:\n        assert (\"x\" in item) or (item in values), \"bad\"\n";
        assert_matches_cpython_marshal(source, "backward_bool.py");
    }

    #[test]
    fn matches_cpython_marshal_for_loop_and_async_iteration_edges() {
        let source = "def f():\n    for value in range(10):\n        if 2 <= value <= 8:\n            print(value)\n\ndef constant_false_loop(value):\n    while False:\n        unreachable()\n    return value\n\ndef constant_true_break(manager):\n    while True:\n        try:\n            break\n        except Exception:\n            pass\n    manager.value = manager.other\n\ndef nested_break_finally():\n    while condition:\n        try:\n            while ():\n                if 3:\n                    break\n        finally:\n            return\n\nasync def async_tail(items, result):\n    async for value in items:\n        if value:\n            result.append(value)\n\nasync def protected_async_for(manager, items):\n    with manager:\n        async for item in items:\n            ...\n\nasync def protected_async_comprehension(manager, items):\n    async with manager:\n        consume({item async for item in items})\n\nasync def nested_async_cleanup(items):\n    async for item in items:\n        await sleep(item)\n        async with manager(item):\n            consume(item)\n\ndef protected_boolean_tail(manager, items):\n    with manager:\n        for item in items:\n            if first(item) and second(item):\n                action(item)\n\ndef nested_boolean_tail(manager, items):\n    with manager:\n        for item in items:\n            if (first(item) and second(item)) or (third(item) and fourth(item)):\n                action(item)\n\ndef protected_with_loop(manager, items):\n    with manager:\n        for item in items:\n            while condition:\n                if first:\n                    action(item)\n                elif second:\n                    other(item)\n\ndef protected_generator_loop(items):\n    for item in items:\n        while condition:\n            if first:\n                yield item\n            elif second:\n                yield item + 1\n\ntry:\n    for item in items:\n        while condition:\n            if first:\n                action()\n            elif second:\n                other()\nexcept ValueError:\n    handle()\nfinally:\n    cleanup()\n";
        assert_matches_cpython_marshal(source, "loop_tail.py");
    }

    #[test]
    fn matches_cpython_marshal_for_terminal_boolean_comprehension() {
        let source = "foo or {x: None for x in bar}\n";
        assert_matches_cpython_marshal(source, "boolean_comprehension.py");
    }

    #[test]
    fn matches_cpython_marshal_for_short_circuit_comprehension_filters() {
        let source = "z = [a for a in range(5) if a or b or c or d and e]\n";
        assert_matches_cpython_marshal(source, "comprehension_filter.py");
    }

    #[test]
    fn matches_cpython_marshal_for_inlined_comprehension_cells() {
        let source = r#"module_result = [lambda: x for x in range(2)]

def captured_targets():
    lists = [lambda: x for x in range(2)]
    sets = {lambda: x for x in range(2)}
    dictionaries = {x: lambda: x for x in range(2)}
    return lists, sets, dictionaries

def unreachable_target():
    if False:
        return [lambda: item for item in range(3)]

def nested_targets(xs, ys):
    return [[lambda: x for x in ys] for x in xs]
"#;
        assert_matches_cpython_marshal(source, "comprehension_cells.py");
    }

    #[test]
    fn matches_cpython_marshal_for_class_scope_super_resolution() {
        let source = r#"class Outer:
    def method(self):
        class Inner:
            super(Outer, self).method()

            def nested(inner_self):
                super(Outer, self).method()


class Annotated:
    def method(self):
        __class__: object
        super


class Shadowed:
    super = factory
    value = super(Shadowed, instance).method()
"#;
        assert_matches_cpython_marshal(source, "class_super.py");
    }

    #[test]
    fn matches_cpython_marshal_for_optimized_generator_calls() {
        let source = "module_any = any(value for value in values)\nmodule_all = all(value for value in values)\nmodule_tuple = tuple(value for value in values)\nany(value for value in values)\nall(value for value in values)\ntuple(value for value in values)\nafter = None\n\n\ndef optimized(values):\n    return (\n        any(value for value in values),\n        all(value for value in values),\n        tuple(value for value in values),\n    )\n\n\ndef rebound(values, any):\n    return any(value for value in values)\n\n\ndef terminal_discard(values):\n    all(value for value in values)\n";
        assert_matches_cpython_marshal(source, "optimized_generator_calls.py");
    }

    #[test]
    fn matches_cpython_marshal_for_branch_result_ownership() {
        let source = "def ternary(name, suffix):\n    return name[:-len(suffix)] if name.endswith(suffix) else name\n\ndef simple(condition, left, right):\n    return left if condition else right\n\ndef boolean(left, right):\n    return left or right\n\ndef nested(condition, name):\n    return name.upper() if condition else name\n\ndef nested_boolean(left, middle, right):\n    return (left and middle) or right, (left or middle) and right\n\ndef folded_annotation(arg: 2 or None | int):\n    pass\n\nlambda_conditional = lambda condition, left, right: left if condition else right\nlambda_boolean = lambda left, right: left or right\nmodule_boolean = (left and middle) or right\n";
        assert_matches_cpython_marshal(source, "ownership.py");
    }

    #[test]
    fn matches_cpython_marshal_for_a_folded_dictionary_comprehension_key() {
        let source = "def folded_key():\n    return {x if True else y: y for x in range(10) for y in range(10)}\n";
        assert_matches_cpython_marshal(source, "folded_key.py");
    }

    #[test]
    fn matches_cpython_marshal_for_a_named_comprehension_target() {
        let source = "[value for value in values]\n{key: (left, right) for key, (left, right) in values}\nmodule_result = {last := value for value in range(2)}\nnested_filter = [(key, value) for key, value in pairs if key in [field.name for field in fields]]\nnested_iterable = [(left, right) for left in values for right in [item.value for item in items]]\ngenerator_assignment = list((captured := value for value in values))\n\ndef f():\n    exponential, base_multiplier = 1, 2\n    hash_map = {\n        (exponential := (exponential * base_multiplier) % 3): i + 1\n        for i in range(2)\n    }\n    return hash_map\n";
        assert_matches_cpython_marshal(source, "named_comprehension.py");
    }

    #[test]
    fn matches_cpython_marshal_for_multiline_conditional_expressions() {
        let source = "value = (\n    'body'\n    if True\n    else 'otherwise'\n)\n\ndef nested():\n    target.attribute = (\n        (first if inner else second)\n        if outer\n        else third\n    )\n";
        assert_matches_cpython_marshal(source, "conditional_expression.py");
    }

    #[test]
    fn matches_cpython_marshal_for_unpacked_class_arguments() {
        let source = "class C1(Generic[T], str, **{'metaclass': type}):\n    ...\n\nclass C2(Generic[T], str, metaclass=type):\n    ...\n\nclass C3(Generic[T], metaclass=type, *[str]):\n    ...\n\nclass C4(x=1, **y):\n    ...\n";
        assert_matches_cpython_marshal(source, "class_arguments.py");
    }

    #[test]
    fn matches_cpython_marshal_for_import_originated_calls() {
        let source = "import sys\n\ndef reimported():\n    import sys\n    sys.exit(1)\n\ndef shadowed(sys):\n    sys.exit(1)\n\ndef only_local():\n    import os\n    os._exit(1)\n";
        assert_matches_cpython_marshal(source, "imported_calls.py");
    }

    #[test]
    fn matches_cpython_marshal_for_a_nested_if_branch_exit() {
        let source = "seed = 0\nif True:\n    body()\ndef between():\n    pass\nreused = True\n\nif a:\n    if b:\n        body()\nelif c:\n    other()\nafter1(); after2(); after3(); after4(); after5(); after6()\n\nif x:\n    if True:\n        body()\nelif y:\n    other()\n\nif False if True else False:\n    body()\nelif True:\n    other()\nelse:\n    unreachable()\n\ndef conditional_in_loop(src, dst):\n    for k, v in src:\n        if True if True else False:\n            dst[k] = v\n\ndef terminating_nested_if(x, y):\n    if x:\n        if y:\n            raise Exception()\n        else:\n            body()\n    else:\n        other()\n    after()\n\ndef nested_if_before_implicit_return(x, y):\n    if x:\n        if y:\n            return\n        else:\n            value = 2\n    else:\n        value = 3\n    return\n\ndef nested_if_with_fallthrough_return(x, y):\n    if x:\n        if y:\n            return y\n    else:\n        return x\n    return None\n\ndef nested_if_before_loop_control(items, condition):\n    for item in items:\n        if item:\n            if condition:\n                value = 1\n            else:\n                value = 2\n            continue\n        else:\n            value = 3\n        continue\n\ndef protected_loop_control(items):\n    for item in items:\n        try:\n            body()\n            if item:\n                break\n        except Exception:\n            recover()\n    for item in items:\n        try:\n            body()\n            continue\n        except Exception:\n            recover()\n\ndef nested_break_then_return(items, condition):\n    for item in items:\n        if item:\n            if condition:\n                break\n        else:\n            return condition\n        return None\n\nif True:\n    if first:\n        one()\n    elif second:\n        two()\n";
        assert_matches_cpython_marshal(source, "nested_if.py");
    }

    #[test]
    fn matches_cpython_marshal_for_an_unreachable_assertion_message() {
        let source = "seed = '\u{a0}'\nassert True, print('hidden', print.__name__, sep=None)\ncondition = 'used'\n\ndef loop_tail_assert(values):\n    for value in values:\n        assert value\n";
        assert_matches_cpython_marshal(source, "assert.py");
    }

    #[test]
    fn matches_cpython_marshal_for_a_formatted_not_equal_t_string() {
        let source = "t\"{3!=4:}\"\n";
        assert_matches_cpython_marshal(source, "t_string.py");
    }

    #[test]
    fn matches_cpython_marshal_for_an_irrefutable_constant_match_guard() {
        let source = "match subject:\n    case value if True:\n        pass\n    case _:\n        unreachable()\nafter()\n";
        assert_matches_cpython_marshal(source, "match.py");
    }

    #[test]
    fn matches_cpython_marshal_for_sequence_wildcard_star() {
        let source = "def prefix(value):\n    match value:\n        case [captured, *_]:\n            return captured\n\ndef suffix(value):\n    match value:\n        case [*_, captured]:\n            return captured\n\ndef both(value):\n    match value:\n        case [first, *_, last]:\n            return first, last\n\ndef nested(value):\n    match value:\n        case [[captured], *_]:\n            return captured\n";
        assert_matches_cpython_marshal(source, "sequence_wildcard.py");
    }

    #[test]
    fn matches_cpython_marshal_for_yield_in_try_finally() {
        let source = "def generator(flag):\n    if flag:\n        try:\n            yield\n        finally:\n            pass\n    else:\n        yield\n\ndef terminating_try_except_finally():\n    try:\n        raise StopIteration\n    except ValueError:\n        yield 1\n    finally:\n        pass\n";
        assert_matches_cpython_marshal(source, "generator.py");
    }

    #[test]
    fn matches_cpython_marshal_for_nested_generator_qualnames() {
        let source = "def nested(groups):\n    return (sum(item for item in group) for group in groups)\n\ndef lambda_inside(values):\n    return ((lambda: value) for value in values)\n\nresult = consume(\n    # leading\n    item for item in values\n    # trailing\n)\n";
        assert_matches_cpython_marshal(source, "nested_generators.py");
    }

    #[test]
    fn matches_cpython_marshal_for_local_borrowing_across_exception_regions() {
        let source = "def try_return():\n    try:\n        value = process()\n        return value\n    except ValueError:\n        pass\n\ndef with_return(manager):\n    with manager:\n        return 1\n\ndef match_subject(provided):\n    match provided:\n        case True:\n            return captured\n\ndef comprehension(collection):\n    return [element for element in collection if element is not None]\n\ndef repeated_checked_load():\n    value = value[value.attribute == 'key']\n\ndef maybe_unbound(condition):\n    if condition:\n        value = 1\n    else:\n        use()\n    return value\n\ndef consecutive_try_imports():\n    try:\n        from first import First\n    except ImportError:\n        from fallback import First\n    First()\n    try:\n        from second import Second\n    except ImportError:\n        from fallback import Second\n    Second()\n\ndef folded_false(value):\n    if value == 1:\n        return 1\n    elif False:\n        return 2\n    elif value == 3:\n        return 3\n\ndef loaded_before_binding():\n    for item in values:\n        use(values, item)\n    values = []\n";
        assert_matches_cpython_marshal(source, "borrowing.py");
    }

    #[test]
    fn matches_cpython_marshal_for_with_passes_and_folded_iterables() {
        let source = "def pass_before_loop(manager, values):\n    with manager as target:\n        pass\n        for value in values:\n            target.write(value)\n\ndef pass_in_loop_and_else(manager, values):\n    with manager as target:\n        for value in values:\n            pass\n            target.write(value)\n        else:\n            pass\n\ndef folded_iterable(manager):\n    with manager as target:\n        for value in (1,) if True else (2,):\n            target.write(value)\n\nwith (\n    manager\n) as target: pass\n";
        assert_matches_cpython_marshal(source, "with_passes.py");
    }

    #[test]
    fn matches_cpython_marshal_for_a_constant_true_comprehension_filter() {
        let source = "value: int\n[x for x in values if True]\n[x for x in values if not None]\n[result for x in values if not predicate]\n{target.attr: None for target.attr in values}\n[\n    result\n    for x in values\n    if True\n]\n[\n    result\n    for x in values\n    if x\n    if not None\n]\n[x for x in values if left and right]\n{x for x in values if lower < x < upper if accepted}\n(x for x in values if left and right if accepted)\n(x for x in values if left and right async for y in async_values if accepted)\n\ndef restored_then_subscript(values):\n    result = {}\n    result['key'] = [value for value in values]\n\ndef assignment_reuses_target():\n    tasks = [task for task in tasks]\n";
        assert_matches_cpython_marshal(source, "comprehension.py");
    }

    #[test]
    fn matches_cpython_marshal_for_folded_tuple_not_nops() {
        let source = "multiline = (\n    not \"a\",\n    not \"b\",\n    (not \"c\",),\n)\nsame_line = (not \"a\", not \"b\")\nother_folds = (1 + 2, -3, ~4)\n";
        assert_matches_cpython_marshal(source, "folded_tuple.py");
    }

    #[test]
    fn matches_cpython_marshal_for_folded_boolean_addition() {
        let source = "seed = 'seed'\nvalue = True + False\n";
        assert_matches_cpython_marshal(source, "folded_boolean_addition.py");
    }

    #[test]
    fn matches_cpython_marshal_for_a_large_constant_list() {
        let source = format!(
            "large = [{}]\n",
            (0..31)
                .map(|value| value.to_string())
                .collect::<Vec<_>>()
                .join(", ")
        );
        assert_matches_cpython_marshal(&source, "large_constant_list.py");
    }

    #[test]
    fn matches_cpython_marshal_for_a_terminal_finally_suite() {
        let source = "\"\"\"doc\"\"\"\ntry:\n    raise ValueError\nfinally:\n    raise RuntimeError\nafter = 1\ntry:\n    action()\nexcept Error as captured:\n    consume(captured)\n";
        assert_matches_cpython_marshal(source, "terminal_finally.py");
    }

    #[test]
    fn matches_cpython_marshal_for_annotation_thunk_edges() {
        let source = "from typing import Annotated, List, TypedDict\n\n()\nvalue: int\n\ndef marker():\n    pass\n\nclass Item(TypedDict):\n    nodes: List[TypedDict('Node', {'name': str})]\n\nclass Metadata:\n    value: Annotated[int, marker()]\n\ndef outer():\n    from pathlib import Path\n\n    class Visitor:\n        def visit(self, value: Path) -> None:\n            pass\n";
        assert_matches_cpython_marshal(source, "annotation.py");
    }

    #[test]
    fn matches_cpython_marshal_for_annotations_in_compound_suites() {
        let source = r#"with manager:
    in_with: WithAnnotation = 1

try:
    in_try: TryAnnotation = 2
except Error:
    in_except: ExceptAnnotation = 3
else:
    in_else: ElseAnnotation = 4
finally:
    in_finally: FinallyAnnotation = 5

match subject:
    case 1:
        in_match: MatchAnnotation = 6
"#;
        assert_matches_cpython_marshal(source, "compound_annotations.py");
    }

    #[test]
    fn matches_cpython_marshal_for_unreachable_deferred_annotations() {
        let source =
            "seed = 0\nraise terminal\nvalue: Annotation = build()\ndeclaration: Annotation\n";
        assert_matches_cpython_marshal(source, "unreachable_annotation.py");
    }

    #[test]
    fn matches_cpython_marshal_for_an_inline_constant_with_body() {
        let source = "import io\nwith (\n    io  # comment\n    .open('file.txt') as file\n): ...\nwith manager:\n    while condition:\n        body()\n";
        assert_matches_cpython_marshal(source, "with.py");
    }

    #[test]
    fn matches_cpython_marshal_for_a_tuple_containing_a_constant_slice() {
        let source = "constant = lambda value: value[:, 1]\ndynamic = lambda value: value[::-1]\nwith_defaults = lambda value=1, other=2: value\nstring_slice = 'héllo'[1:4]\nstepped_string_slice = 'abcdef'[1:5:2]\nboolean_slice = 'abc'[False:True:True]\nbytes_slice = b'prefix'[2:]\ntuple_slice = (1, 2, 3)[1:]\nnegative_slice = 'abc'[:-1]\nstring_index = 'héllo'[1]\nfstring_index = f'abc'[0]\nbytes_index = b'prefix'[-1]\ntuple_index = (1, 2, 3)[-2]\ndynamic_list_index = [1, 2, 3][0]\nmember = item in [1, 2]\ndouble_negation = not not item\n5.0 ** 5.0\nTrue ** True\nFalse ** False\n() << 0 ** 99999999999999999999999999\n1 - 2\ninverted = ~1\nnegative_zero = -0000\nmixed = 1 + 2.5\nassert (False, 'x')\nassert (False,)\nassert ()\nassert True\npacked = fn('User', **{'name': str})\nunicode_keyword = fn(café=1)\nhuge = -999999999999999999999999999999999999999999\nadjacent = ('' f'prefix {item}')\nleft if condition else right\n'%s' % ('value',)\nif (1, 2):\n    pass\n";
        assert_matches_cpython_marshal(source, "example.py");
    }

    #[test]
    fn matches_cpython_marshal_for_lambda_edge_cases() {
        let source = "from __future__ import barry_as_FLUFL, generator_stop, print_function\nsimple = lambda: (yield None)\nyield_value = lambda value: (yield value)\ndelegating = lambda: (yield from source())\ndef eliminated():\n    while False:\n        value = lambda: missing\ndef   trailing_semicolon():\n    value = 1 \\\n        ;\ndef trailing_semicolon_try():\n    try:\n        value = 1;\n    except ValueError:\n        pass;\ndef overridden_return():\n    try:\n        return 1\n    finally:\n        return 2\ndef overridden_handler_return():\n    try:\n        pass\n    except:\n        return 1\n    finally:\n        return 2\ndef overridden_try_and_handler_returns():\n    try:\n        call()\n        return 1\n    except:\n        return 2\n    finally:\n        return 3\ndef assigned_after_try_except_finally():\n    value = None\n    try:\n        call()\n        value = 1\n    except:\n        value = 2\n    finally:\n        value = 3\n    return value\ndef empty_try_else():\n    try:\n        pass\n    except:\n        return 1\n    else:\n        return 2\ndef dead_loop_return(value):\n    while value > 0:\n        break\n        return 1\ndef return_inside_with():\n    with manager:\n        return 1\ndef finally_return_in_loop(items):\n    for item in items:\n        try:\n            use(item)\n        finally:\n            return\nasync  def terminal_async_comprehension():\n    if test:\n        values = [value async for value in source]\n";
        assert_matches_cpython_marshal(source, "example.py");
    }

    #[test]
    fn matches_cpython_marshal_for_a_return_through_finally() {
        let source = "def return_through_finally(items):\n    for item in items:\n        try:\n            pass\n        except Exception:\n            return\n        finally:\n            consume(item)\n\ndef preserve_return_value():\n    try:\n        pass\n        return produce()\n    finally:\n        consume()\n\ndef preserve_try_pass():\n    try:\n        pass\n        return produce()\n    except Exception:\n        recover()\n";
        assert_matches_cpython_marshal(source, "example.py");
    }

    #[test]
    fn matches_cpython_marshal_for_finally_end_control_flow() {
        let source = "def conditional_return(flag):\n    try:\n        pass\n    finally:\n        if flag:\n            return\n\ndef nested_return():\n    try:\n        try:\n            pass\n        finally:\n            return\n    finally:\n        pass\n\ndef nested_handler_return():\n    try:\n        pass\n    finally:\n        try:\n            return\n        except Exception:\n            pass\n\ndef break_from_finally():\n    while True:\n        try:\n            pass\n        finally:\n            break\n\ndef continue_from_finally():\n    while True:\n        try:\n            pass\n        finally:\n            continue\n";
        assert_matches_cpython_marshal(source, "finally_end.py");
    }

    #[test]
    fn matches_cpython_marshal_for_pass_branches_and_surrogates() {
        let source = "def pass_elif(value):\n    if value:\n        pass\n    elif check(value):\n        value = update(value)\n    return value\n\ndef surrogates():\n    explicit_pair = \"\\ud800\\udc00heythere\"\n    lone_surrogate = \"\\ud800\"\n    mixed = \"\u{fffd}\\ud800\"\n    scalar = \"\\U00010000heythere\"\n    return explicit_pair, lone_surrogate, mixed, scalar\n";
        assert_matches_cpython_marshal(source, "example.py");
    }

    #[test]
    fn matches_cpython_marshal_for_a_percent_formatted_fstring() {
        let source = "f\"hello %s %s\" % (1, 2)\n\"%s %s\" % (1, 2)\n";
        assert_matches_cpython_marshal(source, "example.py");
    }

    #[test]
    fn matches_cpython_marshal_for_folded_not_and_terminal_while() {
        let source = "folded = not (\n    True\n)\n\ndef loop():\n    while not check() and left < right:\n        pass\n";
        assert_matches_cpython_marshal(source, "example.py");
    }

    #[test]
    fn executes_arithmetic_names_and_calls() {
        assert_executes("answer = 6 * 7\nprint(answer)\n", "42\n");
    }

    #[test]
    fn executes_control_flow() {
        let source = r#"
x = 3
total = 0
while x:
    total += x
    x -= 1

if total == 6:
    print("ok")
else:
    print("bad")
"#;
        assert_executes(source, "ok\n");
    }

    #[test]
    fn executes_simple_functions() {
        let source = r"
def add(left, right):
    result = left + right
    return result

print(add(20, 22))
";
        assert_executes(source, "42\n");
    }

    #[test]
    fn executes_literals_and_collections() {
        let source = r#"
values = [1, 2, 3]
pair = ("answer", 42)
mapping = {"answer": 42}
print(values[1], pair[0], mapping["answer"])
"#;
        assert_executes(source, "2 answer 42\n");
    }

    #[test]
    fn executes_short_circuit_expressions() {
        let source = r"
zero = 0
one = 1
print(zero and missing, one or missing, 1 if zero else 2, not zero)
";
        assert_executes(source, "0 1 2 True\n");
    }

    #[test]
    fn executes_break_continue_and_while_else() {
        let source = r"
x = 4
total = 0
while x:
    x -= 1
    if x == 2:
        continue
    if x == 0:
        break
    total += x
else:
    total = 999
print(total)
";
        assert_executes(source, "4\n");
    }

    #[test]
    fn executes_function_globals_and_docstrings() {
        let source = r#"
base = 40
def answer(offset):
    "answer docs"
    return base + offset

print(answer(2), answer.__doc__)
"#;
        assert_executes(source, "42 answer docs\n");
    }

    #[test]
    fn executes_extended_arguments_and_large_integers() {
        let mut source = String::from("flag = False\nif flag:\n");
        for index in 0..300 {
            writeln!(&mut source, "    value_{index} = {index}").unwrap();
        }
        source.push_str("else:\n    value_299 = 9223372036854775808\nprint(value_299)\n");

        assert_executes(&source, "9223372036854775808\n");
    }

    #[test]
    fn executes_for_loops_imports_and_globals() {
        let source = r"
import math as mathematics
from operator import add as plus

total = 0
for value in [1, 2, 3]:
    if value == 2:
        continue
    total = plus(total, value)
else:
    total += int(mathematics.sqrt(4))

def bump():
    global total
    total += 1

bump()
print(total)
";
        assert_executes(source, "7\n");
    }

    #[test]
    fn executes_unpacking_stores_and_deletes() {
        let source = r"
from types import SimpleNamespace

box = SimpleNamespace(value=1)
box.value += 4
values = [10, 20, 30]
values[1] += 2
head, *middle, tail = values
box.extra = head + tail
del values[0]
del box.extra
print(box.value, values, head, middle, tail, hasattr(box, 'extra'))
";
        assert_executes(source, "5 [22, 30] 10 [22] 30 False\n");
    }

    #[test]
    fn executes_starred_calls_collections_slices_and_comparisons() {
        let source = r"
def combine(a, b, c):
    return a + b + c

args = (1, 2)
keywords = {'c': 3}
values = [0, *args, 3]
mapping = {'a': 1, **{'b': 2}}
result = combine(*args, **keywords)
print(result, values[1:3], mapping, 0 < result < 10, (captured := result))
";
        assert_executes(source, "6 [1, 2] {'a': 1, 'b': 2} True 6\n");
    }

    #[test]
    fn executes_full_function_signatures_and_decorators() {
        let source = r"
def identity(function):
    return function

@identity
def combine(a, b=2, *args, c=3, **kwargs):
    return a + b + sum(args) + c + kwargs['d']

print(combine(1, 4, 5, 6, c=7, d=8))
";
        assert_executes(source, "31\n");
    }

    #[test]
    fn executes_lambdas_with_full_signatures() {
        let source = r"
combine = lambda a, b=2, *args, c=3, **kwargs: a + b + sum(args) + c + kwargs['d']
print(combine(1, 4, 5, 6, c=7, d=8))
";
        assert_executes(source, "31\n");
    }

    #[test]
    fn executes_nested_closures() {
        let source = r"
def outer(x):
    y = 2
    def middle(z):
        def inner():
            return x + y + z
        return inner
    return middle

print(outer(1)(3)())
";
        assert_executes(source, "6\n");
    }

    #[test]
    fn executes_nonlocal_assignments() {
        let source = r"
def counter():
    value = 0
    def bump():
        nonlocal value
        value += 1
        return value
    return bump

bump = counter()
print(bump(), bump())
";
        assert_executes(source, "1 2\n");
    }

    #[test]
    fn executes_fstrings_and_arbitrary_precision_integers() {
        let source = r#"
value = 1234567890123456789012345678901234567890
width = 6
print(f"value={value!r} suffix={value:0{width}d}")
print(f"{width=}")
"#;
        assert_executes(
            source,
            "value=1234567890123456789012345678901234567890 suffix=1234567890123456789012345678901234567890\nwidth=6\n",
        );
    }

    #[test]
    fn executes_classes_bases_decorators_and_methods() {
        let source = r"
def identity(value):
    return value

class Base:
    base = 40

@identity
class Answer(Base):
    offset = 2
    def value(self):
        return self.base + self.offset

print(Answer().value(), Answer.__firstlineno__, Answer.__static_attributes__)
";
        assert_executes(source, "42 8 ()\n");
    }

    #[test]
    fn executes_deferred_function_annotations() {
        let source = r"
def concrete(value: int) -> str:
    return str(value)

print(concrete.__annotations__)
";
        assert_executes(
            source,
            "{'value': <class 'int'>, 'return': <class 'str'>}\n",
        );

        let source = r"
from __future__ import annotations

def deferred(value: list[int]) -> str:
    return str(value)

print(deferred.__annotations__)
";
        assert_executes(source, "{'value': 'list[int]', 'return': 'str'}\n");
    }

    #[test]
    fn executes_future_module_annotations() {
        let source = r"
from __future__ import annotations

answer: int = 42
values: list[str]
print(__annotations__)
";
        assert_executes(source, "{'answer': 'int', 'values': 'list[str]'}\n");
    }

    #[test]
    fn executes_deferred_module_annotations() {
        let source = r"
answer: int = 42
values: list[str]
print(__annotate__(1))
";
        assert_executes(source, "{'answer': <class 'int'>, 'values': list[str]}\n");
    }

    #[test]
    fn executes_try_except_else_and_named_handlers() {
        let source = r#"
def parse(flag):
    try:
        if flag:
            raise ValueError("bad")
        value = 40
    except ValueError as error:
        value = len(str(error))
    else:
        value += 2
    return value

print(parse(False), parse(True))
"#;
        assert_executes(source, "42 3\n");
    }

    #[test]
    fn executes_try_finally_on_normal_and_exceptional_paths() {
        let source = r#"
events = []
try:
    events.append("normal")
finally:
    events.append("cleanup-1")

try:
    raise ValueError("bad")
except ValueError:
    events.append("handled")
finally:
    events.append("cleanup-2")

def overriding_return():
    try:
        events.append("value")
        return len(events)
    finally:
        events.append("override")
        return 2

def break_from_finally():
    for value in [1, 2]:
        try:
            events.append(("try", value))
        finally:
            events.append(("break", value))
            break
    return "broken"

def continue_from_finally():
    result = []
    for value in [1, 2]:
        try:
            result.append(("try", value))
            raise ValueError(value)
        finally:
            result.append(("continue", value))
            continue
    return result

print(events)
print(overriding_return(), events)
print(break_from_finally(), events[-2:])
print(continue_from_finally())
"#;
        assert_executes(
            source,
            "['normal', 'cleanup-1', 'handled', 'cleanup-2']\n2 ['normal', 'cleanup-1', 'handled', 'cleanup-2', 'value', 'override']\nbroken [('try', 1), ('break', 1)]\n[('try', 1), ('continue', 1), ('try', 2), ('continue', 2)]\n",
        );
    }

    #[test]
    fn executes_context_managers_and_exception_suppression() {
        let source = r#"
events = []
class Manager:
    def __init__(self, suppress):
        self.suppress = suppress
    def __enter__(self):
        events.append("enter")
        return 42
    def __exit__(self, kind, value, traceback):
        events.append("exit")
        return self.suppress

with Manager(False) as value:
    events.append(value)

with Manager(True):
    raise ValueError("suppressed")

with Manager(False), Manager(True):
    raise ValueError("suppressed by inner manager")

for index in range(3):
    with Manager(False):
        events.append(index)
        if index == 0:
            continue
        break

for index in range(2):
    try:
        raise ValueError(index)
    except ValueError:
        events.append(f"handled-{index}")
        continue

print(events)
"#;
        assert_executes(
            source,
            "['enter', 42, 'exit', 'enter', 'exit', 'enter', 'enter', 'exit', 'exit', 'enter', 0, 'exit', 'enter', 1, 'exit', 'handled-0', 'handled-1']\n",
        );
    }

    #[test]
    fn executes_inlined_comprehensions_without_leaking_targets() {
        let source = r"
x = 100
values = [x * 2 for x in [-1, 1, 2] if x > 0]
pairs = {(x, y) for x in [1, 2] for y in [3, 4] if y > 3}
mapping = {x: x * 2 for x in [1, 2]}
print(values, sorted(pairs), mapping, x)
";
        assert_executes(source, "[2, 4] [(1, 4), (2, 4)] {1: 2, 2: 4} 100\n");
    }

    #[test]
    fn executes_generators_and_generator_expressions() {
        let source = r"
def numbers():
    yield 1
    yield 2

def all_numbers():
    yield from numbers()

def capture():
    list((last := value) for value in numbers())
    return last

yielding_lambda = lambda: (yield 3)
delegating_lambda = lambda: (yield from numbers())

generated = (value * 2 for value in numbers() if value > 1)
print(list(all_numbers()), list(generated), capture(), list(yielding_lambda()), list(delegating_lambda()))
";
        assert_executes(source, "[1, 2] [4] 2 [3] [1, 2]\n");
    }

    #[test]
    fn executes_coroutines_and_async_generators() {
        let source = r"
import asyncio

async def child():
    return 42

async def numbers():
    yield 1
    yield 2

async def main():
    iterator = numbers()
    return await child(), await anext(iterator), await anext(iterator)

async def collect(stop):
    result = []
    async for number in numbers():
        if number == stop:
            break
        result.append(number)
    else:
        result.append('done')
    return result

events = []

class AsyncManager:
    async def __aenter__(self):
        events.append('enter')
        return 42

    async def __aexit__(self, typ, value, traceback):
        events.append(typ.__name__ if typ else 'exit')
        return typ is ValueError

async def contexts():
    async with AsyncManager() as value:
        events.append(value)
    async with AsyncManager():
        raise ValueError('suppressed')
    return events

async def return_from_context():
    async with AsyncManager() as value:
        return value + 1

async def loop_contexts():
    values = []
    for index in range(3):
        async with AsyncManager() as value:
            values.append(value)
            if index == 0:
                continue
            break
    return values

async def comprehensions():
    generated = (number async for number in numbers())
    return (
        [number * 2 async for number in numbers() if number > 1],
        {number async for number in numbers()},
        {number: number * 2 async for number in numbers()},
        [number async for number in generated],
    )

def awaited_generator():
    return (await child() for _ in [0, 1])

async def consume_awaited_generator():
    return [value async for value in awaited_generator()]

print(
    asyncio.run(main()),
    asyncio.run(collect(99)),
    asyncio.run(collect(2)),
    asyncio.run(contexts()),
    asyncio.run(comprehensions()),
    asyncio.run(consume_awaited_generator()),
)
print(
    asyncio.run(return_from_context()),
    events[-2:],
    asyncio.run(loop_contexts()),
    events[-4:],
)
";
        assert_executes(
            source,
            "(42, 1, 2) [1, 2, 'done'] [1] ['enter', 42, 'exit', 'enter', 'ValueError'] ([4], {1, 2}, {1: 2, 2: 4}, [1, 2]) [42, 42]\n43 ['enter', 'exit'] [42, 42] ['enter', 'exit', 'enter', 'exit']\n",
        );
    }

    #[test]
    fn executes_structural_pattern_matching() {
        let source = r"
class Point:
    __match_args__ = ('x', 'y')

    def __init__(self, x, y):
        self.x = x
        self.y = y

def classify(value):
    match value:
        case None:
            return 'none'
        case (1 | 2) as number:
            return ('number', number)
        case {'x': x, **rest}:
            return ('mapping', x, rest)
        case [first, *middle, last] if first < last:
            return ('sequence', first, middle, last)
        case Point(x, y=0):
            return ('point', x)
        case _:
            return 'other'

print(classify(None))
print(classify(2))
print(classify({'x': 1, 'y': 2}))
print(classify([1, 2, 3, 4]))
print(classify(Point(5, 0)))
print(classify(object()))
";
        assert_executes(
            source,
            "none\n('number', 2)\n('mapping', 1, {'y': 2})\n('sequence', 1, [2, 3], 4)\n('point', 5)\nother\n",
        );
    }

    #[test]
    fn executes_template_strings() {
        let source = r"
name = 'world'
width = 8
template = t'hello {name!r:>{width}}!'
interpolation = template.interpolations[0]
print(template.strings)
print(interpolation.value, interpolation.expression, interpolation.conversion, interpolation.format_spec)
";
        assert_executes(source, "('hello ', '!')\nworld name r >8\n");
    }

    #[test]
    fn executes_starred_annotations_and_class_arguments() {
        let source = r"
bases = (object,)
options = {}

class Dynamic(*bases, **options):
    marker = 42

def annotated(*args: *tuple[int]):
    pass

print(Dynamic.marker, annotated.__annotate__(1))
";
        assert_executes(source, "42 {'args': *tuple[int]}\n");
    }

    #[test]
    fn executes_type_aliases_and_generic_functions() {
        let source = r"
type Plain = list[int]
type Pair[T] = tuple[T, T]

def identity[T](value: T) -> T:
    return value

def bounded[T: int = bool](value: T) -> T:
    return value

class Box[T]:
    def __init__(self, value):
        self.value = value

print(Plain.__value__, Plain.__type_params__)
print(Pair.__type_params__, Pair[int], Pair[int].__value__)
print(identity(42), identity.__type_params__, identity.__annotate__(1))
print(bounded.__type_params__[0].__bound__, bounded.__type_params__[0].__default__)
print(Box.__type_params__, Box[int], Box[int](42).value)
";
        assert_executes(
            source,
            "list[int] ()\n(T,) Pair[int] tuple[T, T]\n42 (T,) {'value': T, 'return': T}\n<class 'int'> <class 'bool'>\n(T,) __main__.Box[int] 42\n",
        );
    }
}
