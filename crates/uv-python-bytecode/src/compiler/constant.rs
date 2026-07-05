// Compiler submodules intentionally share the parent-owned state and lowering vocabulary.
#![allow(clippy::wildcard_imports)]

use super::*;

impl Compiler {
    pub(super) fn emit_folded_constant(
        &mut self,
        expression: &Expr,
        constant: Constant,
    ) -> Result<(), CompileError> {
        if matches!(expression, Expr::Name(name) if name.id.as_str() == "__debug__") {
            return self.emit_preprocessed_constant(expression, constant);
        }
        if let Some(literal) = literal_constant(expression) {
            self.add_constant(literal)?;
        } else {
            self.record_folded_operands(expression)?;
        }
        if matches!(expression, Expr::Tuple(_))
            || matches!(expression, Expr::UnaryOp(unary) if unary.op == UnaryOp::Not)
        {
            self.emit_folded_tuple_not_nops(expression)?;
            self.assembler
                .set_location(self.source_location(expression.range()));
        }
        if let Constant::Int(value) = constant
            && let Ok(value) = u8::try_from(value)
        {
            self.emit(LOAD_SMALL_INT, u32::from(value), 1)
        } else {
            self.emit_deferred_constant(constant)
        }
    }

    fn record_folded_operands(&mut self, expression: &Expr) -> Result<(), CompileError> {
        match expression {
            Expr::BinOp(binary) => {
                self.record_folded_value(&binary.left)?;
                self.record_folded_value(&binary.right)?;
            }
            Expr::Tuple(tuple) => {
                for element in &tuple.elts {
                    self.record_folded_value(element)?;
                }
            }
            Expr::Subscript(subscript) => {
                self.record_folded_value(&subscript.value)?;
                if let Expr::Slice(slice) = subscript.slice.as_ref() {
                    if let Some(constant) = constant_slice(slice) {
                        self.add_constant(constant)?;
                    } else {
                        for bound in [slice.lower.as_deref(), slice.upper.as_deref()]
                            .into_iter()
                            .flatten()
                        {
                            self.record_folded_value(bound)?;
                        }
                    }
                } else if !matches!(
                    fold_constant(&subscript.slice),
                    Some(Constant::Int(value)) if u8::try_from(value).is_ok()
                ) {
                    self.record_folded_value(&subscript.slice)?;
                }
            }
            Expr::UnaryOp(unary) => self.record_folded_value(&unary.operand)?,
            _ => {}
        }
        Ok(())
    }

    /// Emits operand-load residue retained when CPython's flowgraph folds a tuple containing
    /// constant boolean negations.
    pub(super) fn emit_folded_tuple_not_nops(
        &mut self,
        expression: &Expr,
    ) -> Result<(), CompileError> {
        match expression {
            Expr::Tuple(tuple) => {
                for element in &tuple.elts {
                    self.emit_folded_tuple_not_nops(element)?;
                }
            }
            Expr::UnaryOp(unary) if unary.op == UnaryOp::Not => {
                self.assembler
                    .set_location(self.source_location(unary.operand.range()));
                self.emit(NOP, 0, 0)?;
            }
            _ => {}
        }
        Ok(())
    }

    pub(super) fn record_folded_value(&mut self, expression: &Expr) -> Result<(), CompileError> {
        if let Some(constant) = literal_constant(expression) {
            self.add_constant(constant)?;
            return Ok(());
        }
        if matches!(expression, Expr::Slice(_))
            && let Some(constant) = fold_constant(expression)
        {
            self.add_constant(constant)?;
            return Ok(());
        }
        if matches!(expression, Expr::Name(name) if name.id.as_str() == "__debug__") {
            self.add_constant(Constant::Bool(true))?;
            return Ok(());
        }
        if matches!(expression, Expr::FString(_))
            && let Some(constant) = fold_constant(expression)
        {
            self.add_constant(constant)?;
            return Ok(());
        }

        self.record_folded_operands(expression)?;
        if let Some(constant) = fold_constant(expression)
            && !matches!(constant, Constant::Int(value) if u8::try_from(value).is_ok())
        {
            self.output.deferred_constants.push((None, constant));
        }
        Ok(())
    }

    pub(super) fn emit_preprocessed_constant(
        &mut self,
        expression: &Expr,
        constant: Constant,
    ) -> Result<(), CompileError> {
        if let Constant::Int(value) = constant
            && let Ok(value) = u8::try_from(value)
        {
            if self.output.constants.is_empty()
                && let Some(seed) = first_literal_constant(expression)
            {
                self.add_constant(seed)?;
            }
            self.emit(LOAD_SMALL_INT, u32::from(value), 1)
        } else {
            let index = self.add_constant(constant)?;
            self.emit(LOAD_CONST, index, 1)
        }
    }

    pub(super) fn name_index(&mut self, name: &str) -> Result<u32, CompileError> {
        let name = self.mangled_name(name);
        let name_id = self.required_name_id(&name);
        if let Some(index) = self.output.name_indices.get(&name_id) {
            return Ok(*index);
        }
        let index = to_u32(self.output.names.len(), "name count")?;
        self.output.names.push(name);
        self.output.name_indices.insert(name_id, index);
        Ok(index)
    }

    pub(super) fn add_constant(&mut self, constant: Constant) -> Result<u32, CompileError> {
        if let Some(index) = self
            .output
            .constants
            .iter()
            .position(|existing| constants_equal(existing, &constant))
        {
            return to_u32(index, "constant index");
        }
        let index = to_u32(self.output.constants.len(), "constant count")?;
        self.output.constants.push(constant);
        Ok(index)
    }

    pub(super) fn add_interned_string_tuple(
        &mut self,
        values: Vec<Constant>,
    ) -> Result<u32, CompileError> {
        self.output
            .interned_constant_strings
            .extend(values.iter().filter_map(|value| match value {
                Constant::String(value) => Some(value.clone()),
                _ => None,
            }));
        self.add_constant(Constant::Tuple(values))
    }

    pub(super) fn remove_unused_constants(&mut self) -> Result<(), CompileError> {
        let used = self.assembler.used_constant_indices(LOAD_CONST);
        let mut index_map = vec![None; self.output.constants.len()];
        let mut retained = Vec::with_capacity(self.output.constants.len());
        for (old_index, constant) in std::mem::take(&mut self.output.constants)
            .into_iter()
            .enumerate()
        {
            if old_index == 0 || used.contains(&u32::try_from(old_index).unwrap()) {
                let new_index = to_u32(retained.len(), "constant count")?;
                index_map[old_index] = Some(new_index);
                retained.push(constant);
            }
        }
        self.assembler
            .remap_constant_indices(LOAD_CONST, &index_map);
        self.output.constants = retained;
        Ok(())
    }

    pub(super) fn emit_deferred_constant(
        &mut self,
        constant: Constant,
    ) -> Result<(), CompileError> {
        self.apply_stack_effect(1)?;
        let instruction = self
            .assembler
            .emit_placeholder_with_depth(LOAD_CONST, self.control.depth.cast_unsigned());
        self.output
            .deferred_constants
            .push((Some(instruction), constant));
        Ok(())
    }

    pub(super) fn emit_deferred_constant_before_return(
        &mut self,
        constant: Constant,
    ) -> Result<(), CompileError> {
        self.apply_stack_effect(1)?;
        let instruction = self
            .assembler
            .emit_placeholder_with_depth(LOAD_CONST, self.control.depth.cast_unsigned());
        self.output
            .deferred_constants_before_return
            .push((instruction, constant));
        Ok(())
    }

    pub(super) fn emit_deferred_store_name(&mut self, name: &str) -> Result<(), CompileError> {
        self.apply_stack_effect(-1)?;
        let instruction = self
            .assembler
            .emit_placeholder_with_depth(STORE_NAME, self.control.depth.cast_unsigned());
        self.output
            .deferred_names
            .push((instruction, name.to_string()));
        Ok(())
    }
}

pub(super) fn explicit_surrogate_string(value: &str, source: &str) -> Option<Vec<u8>> {
    let source = source.as_bytes();
    // Ruff stores invalid Unicode escapes as replacement characters. Retain their source order
    // relative to real replacement characters so the marshal layer can restore surrogate-pass
    // UTF-8 without changing a neighboring U+FFFD.
    let replacement_character = [0xef, 0xbf, 0xbd];
    let mut replacements = Vec::new();
    let mut index = 0;
    while index < source.len() {
        if source[index..].starts_with(&replacement_character) {
            replacements.push(None);
            index += replacement_character.len();
            continue;
        }
        if source[index] != b'\\' {
            index += 1;
            continue;
        }
        let preceding_backslashes = source[..index]
            .iter()
            .rev()
            .take_while(|byte| **byte == b'\\')
            .count();
        if preceding_backslashes % 2 == 1 {
            index += 1;
            continue;
        }
        let (digit_count, escape_len) = match source.get(index + 1) {
            Some(b'u') => (4, 6),
            Some(b'U') => (8, 10),
            _ => {
                index += 1;
                continue;
            }
        };
        let Some(end) = index
            .checked_add(escape_len)
            .filter(|end| *end <= source.len())
        else {
            index += 1;
            continue;
        };
        let Some(codepoint) = std::str::from_utf8(&source[index + 2..index + 2 + digit_count])
            .ok()
            .and_then(|digits| u32::from_str_radix(digits, 16).ok())
        else {
            index += 1;
            continue;
        };
        if (0xd800..=0xdfff).contains(&codepoint) {
            replacements.push(Some(
                u16::try_from(codepoint).expect("surrogate code point fits in u16"),
            ));
        } else if codepoint == 0xfffd {
            replacements.push(None);
        }
        index = end;
    }
    if !replacements.iter().any(Option::is_some) {
        return None;
    }

    let mut bytes = value.as_bytes().to_vec();
    let mut search_start = 0;
    let mut replaced = false;
    for surrogate in replacements {
        let Some(offset) = bytes[search_start..]
            .windows(replacement_character.len())
            .position(|window| window == replacement_character)
            .map(|offset| search_start + offset)
        else {
            continue;
        };
        let Some(surrogate) = surrogate else {
            search_start = offset + replacement_character.len();
            continue;
        };
        let surrogate_bytes = [
            0xe0 | (surrogate >> 12) as u8,
            0x80 | ((surrogate >> 6) & 0x3f) as u8,
            0x80 | (surrogate & 0x3f) as u8,
        ];
        bytes.splice(
            offset..offset + replacement_character.len(),
            surrogate_bytes,
        );
        search_start = offset + surrogate_bytes.len();
        replaced = true;
    }
    replaced.then_some(bytes)
}

fn constants_equal(left: &Constant, right: &Constant) -> bool {
    match (left, right) {
        (Constant::None, Constant::None) | (Constant::Ellipsis, Constant::Ellipsis) => true,
        (Constant::Bool(left), Constant::Bool(right)) => left == right,
        (Constant::Int(left), Constant::Int(right)) => left == right,
        (Constant::SignedInt(left), Constant::SignedInt(right)) => left == right,
        (
            Constant::BigInt {
                negative: left_negative,
                digits: left_digits,
            },
            Constant::BigInt {
                negative: right_negative,
                digits: right_digits,
            },
        ) => left_negative == right_negative && left_digits == right_digits,
        (Constant::Float(left), Constant::Float(right)) => left.to_bits() == right.to_bits(),
        (
            Constant::Complex {
                real: left_real,
                imag: left_imag,
            },
            Constant::Complex {
                real: right_real,
                imag: right_imag,
            },
        ) => {
            left_real.to_bits() == right_real.to_bits()
                && left_imag.to_bits() == right_imag.to_bits()
        }
        (Constant::String(left), Constant::String(right)) => left == right,
        (Constant::SurrogateString(left), Constant::SurrogateString(right)) => left == right,
        (Constant::Bytes(left), Constant::Bytes(right)) => left == right,
        (Constant::Tuple(left), Constant::Tuple(right)) => {
            left.len() == right.len()
                && left
                    .iter()
                    .zip(right)
                    .all(|(left, right)| constants_equal(left, right))
        }
        (Constant::FrozenSet(left), Constant::FrozenSet(right)) => {
            left.len() == right.len()
                && left
                    .iter()
                    .all(|left| right.iter().any(|right| constants_equal(left, right)))
        }
        (
            Constant::Slice {
                lower: left_lower,
                upper: left_upper,
                step: left_step,
            },
            Constant::Slice {
                lower: right_lower,
                upper: right_upper,
                step: right_step,
            },
        ) => {
            constants_equal(left_lower, right_lower)
                && constants_equal(left_upper, right_upper)
                && constants_equal(left_step, right_step)
        }
        (Constant::Code(left), Constant::Code(right)) => code_objects_equal(left, right),
        (_, _) => false,
    }
}

#[derive(Debug)]
enum NumericReal {
    Integer { negative: bool, digits: Vec<u16> },
    Float(f64),
}

#[expect(
    clippy::float_cmp,
    reason = "CPython constant deduplication uses exact numeric equality"
)]
pub(super) fn python_constants_equal(left: &Constant, right: &Constant) -> bool {
    if let (Some((left_real, left_imag)), Some((right_real, right_imag))) =
        (numeric_parts(left), numeric_parts(right))
    {
        return left_imag == right_imag && numeric_reals_equal(&left_real, &right_real);
    }
    match (left, right) {
        (Constant::Tuple(left), Constant::Tuple(right)) => {
            left.len() == right.len()
                && left
                    .iter()
                    .zip(right)
                    .all(|(left, right)| python_constants_equal(left, right))
        }
        (Constant::FrozenSet(left), Constant::FrozenSet(right)) => {
            left.len() == right.len()
                && left.iter().all(|left| {
                    right
                        .iter()
                        .any(|right| python_constants_equal(left, right))
                })
        }
        _ => constants_equal(left, right),
    }
}

fn numeric_parts(value: &Constant) -> Option<(NumericReal, f64)> {
    match value {
        Constant::Bool(value) => Some((integer_numeric_real(u64::from(*value), false), 0.0)),
        Constant::Int(value) => Some((integer_numeric_real(*value, false), 0.0)),
        Constant::SignedInt(value) => Some((
            integer_numeric_real(value.unsigned_abs(), value.is_negative()),
            0.0,
        )),
        Constant::BigInt { negative, digits } => Some((
            NumericReal::Integer {
                negative: *negative,
                digits: normalized_digits(digits.clone()),
            },
            0.0,
        )),
        Constant::Float(value) => Some((NumericReal::Float(*value), 0.0)),
        Constant::Complex { real, imag } => Some((NumericReal::Float(*real), *imag)),
        _ => None,
    }
}

fn integer_numeric_real(mut value: u64, negative: bool) -> NumericReal {
    let mut digits = Vec::new();
    while value != 0 {
        digits.push((value & 0x7fff) as u16);
        value >>= 15;
    }
    NumericReal::Integer {
        negative: negative && !digits.is_empty(),
        digits,
    }
}

fn normalized_digits(mut digits: Vec<u16>) -> Vec<u16> {
    while digits.last() == Some(&0) {
        digits.pop();
    }
    digits
}

#[expect(
    clippy::float_cmp,
    reason = "CPython constant deduplication uses exact numeric equality"
)]
fn numeric_reals_equal(left: &NumericReal, right: &NumericReal) -> bool {
    match (left, right) {
        (NumericReal::Float(left), NumericReal::Float(right)) => left == right,
        (
            NumericReal::Integer {
                negative: left_negative,
                digits: left_digits,
            },
            NumericReal::Integer {
                negative: right_negative,
                digits: right_digits,
            },
        ) => left_negative == right_negative && left_digits == right_digits,
        (integer @ NumericReal::Integer { .. }, NumericReal::Float(float))
        | (NumericReal::Float(float), integer @ NumericReal::Integer { .. }) => {
            float_integer_numeric_real(*float)
                .is_some_and(|float_integer| numeric_reals_equal(integer, &float_integer))
        }
    }
}

fn float_integer_numeric_real(value: f64) -> Option<NumericReal> {
    if !value.is_finite() {
        return None;
    }
    if value == 0.0 {
        return Some(integer_numeric_real(0, false));
    }

    let bits = value.to_bits();
    let negative = bits >> 63 != 0;
    let exponent_bits = ((bits >> 52) & 0x7ff) as i32;
    let fraction = bits & ((1_u64 << 52) - 1);
    let (mut mantissa, exponent) = if exponent_bits == 0 {
        (fraction, -1074)
    } else {
        ((1_u64 << 52) | fraction, exponent_bits - 1023 - 52)
    };

    if exponent < 0 {
        let shift = u32::try_from(-exponent).ok()?;
        if shift >= 64 || mantissa & ((1_u64 << shift) - 1) != 0 {
            return None;
        }
        mantissa >>= shift;
        return Some(integer_numeric_real(mantissa, negative));
    }

    let mut digits = match integer_numeric_real(mantissa, negative) {
        NumericReal::Integer { digits, .. } => digits,
        NumericReal::Float(_) => unreachable!(),
    };
    let shift = u32::try_from(exponent).ok()?;
    let digit_shift = usize::try_from(shift / 15).ok()?;
    let bit_shift = shift % 15;
    if digit_shift > 0 {
        digits.splice(0..0, std::iter::repeat_n(0, digit_shift));
    }
    if bit_shift > 0 {
        let mut carry = 0_u32;
        for digit in &mut digits {
            let shifted = (u32::from(*digit) << bit_shift) | carry;
            *digit = (shifted & 0x7fff) as u16;
            carry = shifted >> 15;
        }
        if carry != 0 {
            digits.push(u16::try_from(carry).expect("base-2^15 carry fits in u16"));
        }
    }
    Some(NumericReal::Integer { negative, digits })
}

fn code_objects_equal(left: &CodeObject, right: &CodeObject) -> bool {
    left.arg_count == right.arg_count
        && left.positional_only_arg_count == right.positional_only_arg_count
        && left.keyword_only_arg_count == right.keyword_only_arg_count
        && left.stack_size == right.stack_size
        && left.flags == right.flags
        && left.bytecode == right.bytecode
        && left.constants.len() == right.constants.len()
        && left
            .constants
            .iter()
            .zip(&right.constants)
            .all(|(left, right)| constants_equal(left, right))
        && left.names == right.names
        && left.locals == right.locals
        && left.local_kinds == right.local_kinds
        && left.filename == right.filename
        && left.name == right.name
        && left.qualified_name == right.qualified_name
        && left.first_line_number == right.first_line_number
        && left.line_table == right.line_table
        && left.exception_table == right.exception_table
        && left.annotation_thunk == right.annotation_thunk
}
pub(super) fn literal_constant(expression: &Expr) -> Option<Constant> {
    match expression {
        Expr::NoneLiteral(_) => Some(Constant::None),
        Expr::BooleanLiteral(boolean) => Some(Constant::Bool(boolean.value)),
        Expr::EllipsisLiteral(_) => Some(Constant::Ellipsis),
        Expr::NumberLiteral(number) => Some(match &number.value {
            Number::Int(value) => value.as_u64().map_or_else(
                || Constant::BigInt {
                    negative: false,
                    digits: big_integer_digits(&value.to_string()),
                },
                Constant::Int,
            ),
            Number::Float(value) => Constant::Float(*value),
            Number::Complex { real, imag } => Constant::Complex {
                real: *real,
                imag: *imag,
            },
        }),
        Expr::StringLiteral(string) => Some(Constant::String(string.value.to_str().to_string())),
        Expr::BytesLiteral(bytes) => Some(Constant::Bytes(bytes.value.bytes().collect())),
        _ => None,
    }
}
pub(super) enum PercentFormatPart<'a> {
    Literal(String),
    Formatted {
        expression: &'a Expr,
        conversion: Conversion,
        format_spec: Option<String>,
    },
}

pub(super) fn optimized_percent_format(
    expression: &ExprBinOp,
) -> Option<Vec<PercentFormatPart<'_>>> {
    if expression.op != Operator::Mod {
        return None;
    }
    let Expr::StringLiteral(format) = expression.left.as_ref() else {
        return None;
    };
    let Expr::Tuple(arguments) = expression.right.as_ref() else {
        return None;
    };
    if arguments
        .elts
        .iter()
        .any(|argument| matches!(argument, Expr::Starred(_)))
    {
        return None;
    }

    let format = format.value.chars().collect::<Vec<_>>();
    let mut position = 0;
    let mut argument_index = 0;
    let mut parts = Vec::with_capacity(arguments.elts.len() * 2 + 1);
    while position < format.len() {
        let mut literal = String::new();
        while position < format.len() {
            if format[position] != '%' {
                literal.push(format[position]);
                position += 1;
            } else if format.get(position + 1) == Some(&'%') {
                literal.push('%');
                position += 2;
            } else {
                break;
            }
        }
        if !literal.is_empty() {
            parts.push(PercentFormatPart::Literal(literal));
        }
        if position == format.len() {
            break;
        }

        let argument = arguments.elts.get(argument_index)?;
        argument_index += 1;
        position += 1;
        let (conversion, format_spec) = parse_percent_format(&format, &mut position)?;
        parts.push(PercentFormatPart::Formatted {
            expression: argument,
            conversion,
            format_spec,
        });
    }
    (argument_index == arguments.elts.len()).then_some(parts)
}

fn parse_percent_format(
    format: &[char],
    position: &mut usize,
) -> Option<(Conversion, Option<String>)> {
    const MAX_DIGITS: usize = 3;

    let mut next = || {
        let character = format.get(*position).copied()?;
        *position += 1;
        Some(character)
    };

    let mut left_justify = false;
    let mut character = loop {
        let character = next()?;
        match character {
            '-' => left_justify = true,
            '+' | ' ' | '#' | '0' => {}
            _ => break character,
        }
    };

    let mut width = None;
    if character.is_ascii_digit() {
        let mut value = 0_u32;
        let mut digits = 0;
        while character.is_ascii_digit() {
            value = value * 10 + character.to_digit(10).unwrap();
            character = next()?;
            digits += 1;
            if digits >= MAX_DIGITS {
                return None;
            }
        }
        width = Some(value);
    }

    let mut precision = None;
    if character == '.' {
        character = next()?;
        let mut value = 0_u32;
        if character.is_ascii_digit() {
            let mut digits = 0;
            while character.is_ascii_digit() {
                value = value * 10 + character.to_digit(10).unwrap();
                character = next()?;
                digits += 1;
                if digits >= MAX_DIGITS {
                    return None;
                }
            }
        }
        precision = Some(value);
    }

    let conversion = match character {
        's' => Conversion::STR,
        'r' => Conversion::REPR,
        'a' => Conversion::ASCII,
        _ => return None,
    };
    let mut format_spec = String::new();
    if !left_justify && width.is_some_and(|width| width > 0) {
        format_spec.push('>');
    }
    if let Some(width) = width {
        use std::fmt::Write;
        write!(format_spec, "{width}").unwrap();
    }
    if let Some(precision) = precision {
        use std::fmt::Write;
        write!(format_spec, ".{precision}").unwrap();
    }

    Some((conversion, (!format_spec.is_empty()).then_some(format_spec)))
}

#[expect(
    clippy::cast_precision_loss,
    reason = "CPython constant folding intentionally rounds integer operands to binary64"
)]
pub(super) fn fold_constant(expression: &Expr) -> Option<Constant> {
    if let Some(constant) = literal_constant(expression) {
        return Some(constant);
    }
    match expression {
        Expr::Name(name) if name.ctx == ExprContext::Load && name.id.as_str() == "__debug__" => {
            Some(Constant::Bool(true))
        }
        Expr::FString(fstring) => {
            let mut value = String::new();
            for part in &fstring.value {
                match part {
                    FStringPart::Literal(literal) => value.push_str(&literal.value),
                    FStringPart::FString(fstring) => {
                        for element in &fstring.elements {
                            let InterpolatedStringElement::Literal(literal) = element else {
                                return None;
                            };
                            value.push_str(&literal.value);
                        }
                    }
                }
            }
            Some(Constant::String(value))
        }
        Expr::Tuple(tuple) if tuple.ctx == ExprContext::Load => tuple
            .elts
            .iter()
            .map(fold_constant)
            .collect::<Option<Vec<_>>>()
            .map(Constant::Tuple),
        Expr::Subscript(subscript) if subscript.ctx == ExprContext::Load => {
            fold_literal_subscript(&subscript.value, &subscript.slice)
        }
        Expr::Slice(slice) => constant_slice(slice),
        Expr::UnaryOp(unary) => match unary.op {
            UnaryOp::UAdd => fold_constant(&unary.operand),
            UnaryOp::Not => literal_truthiness(&unary.operand).map(|value| Constant::Bool(!value)),
            UnaryOp::USub => match fold_constant(&unary.operand)? {
                Constant::Int(0) => Some(Constant::Int(0)),
                Constant::Int(value) => i64::try_from(value)
                    .ok()
                    .map(|value| Constant::SignedInt(-value)),
                Constant::BigInt { negative, digits } => Some(Constant::BigInt {
                    negative: !negative,
                    digits,
                }),
                Constant::Float(value) => Some(Constant::Float(-value)),
                Constant::Complex { real, imag } => Some(Constant::Complex {
                    real: -real,
                    imag: -imag,
                }),
                _ => None,
            },
            UnaryOp::Invert => match fold_constant(&unary.operand)? {
                Constant::Int(value) => {
                    let magnitude = u128::from(value) + 1;
                    match magnitude.cmp(&u128::from(i64::MIN.unsigned_abs())) {
                        std::cmp::Ordering::Less => Some(Constant::SignedInt(
                            -i64::try_from(magnitude).expect("magnitude below 2^63 fits in i64"),
                        )),
                        std::cmp::Ordering::Equal => Some(Constant::SignedInt(i64::MIN)),
                        std::cmp::Ordering::Greater => Some(Constant::BigInt {
                            negative: true,
                            digits: big_integer_digits(&magnitude.to_string()),
                        }),
                    }
                }
                Constant::SignedInt(value) => Some(Constant::SignedInt(!value)),
                _ => None,
            },
        },
        Expr::BinOp(binary) => {
            let left = fold_constant(&binary.left)?;
            let right = fold_constant(&binary.right)?;
            match (left, binary.op, right) {
                (Constant::Bool(left), Operator::Add, Constant::Bool(right)) => {
                    Some(Constant::Int(u64::from(left) + u64::from(right)))
                }
                (Constant::Int(left), Operator::Add, Constant::Int(right)) => {
                    left.checked_add(right).map(Constant::Int)
                }
                (
                    left @ (Constant::Int(_)
                    | Constant::BigInt {
                        negative: false, ..
                    }),
                    Operator::Add,
                    right @ (Constant::Int(_)
                    | Constant::BigInt {
                        negative: false, ..
                    }),
                ) => fold_positive_integer_add(&left, &right),
                (Constant::Int(left), Operator::Sub, Constant::Int(right)) => {
                    if let Some(value) = left.checked_sub(right) {
                        Some(Constant::Int(value))
                    } else {
                        let magnitude = right - left;
                        if magnitude <= i64::MAX.cast_unsigned() {
                            Some(Constant::SignedInt(-(magnitude.cast_signed())))
                        } else if magnitude == i64::MIN.unsigned_abs() {
                            Some(Constant::SignedInt(i64::MIN))
                        } else {
                            Some(Constant::BigInt {
                                negative: true,
                                digits: big_integer_digits(&magnitude.to_string()),
                            })
                        }
                    }
                }
                (Constant::Int(left), Operator::Mult, Constant::Int(right)) => {
                    left.checked_mul(right).map(Constant::Int)
                }
                (Constant::Int(left), Operator::Div, Constant::Int(right)) if right != 0 => {
                    Some(Constant::Float(left as f64 / right as f64))
                }
                (Constant::Int(left), Operator::FloorDiv, Constant::Int(right)) if right != 0 => {
                    Some(Constant::Int(left / right))
                }
                (Constant::Int(left), Operator::Mod, Constant::Int(right)) if right != 0 => {
                    Some(Constant::Int(left % right))
                }
                (Constant::Int(left), Operator::Pow, Constant::Int(right)) => {
                    let right = u32::try_from(right).ok()?;
                    if let Some(value) = left.checked_pow(right) {
                        Some(Constant::Int(value))
                    } else {
                        u128::from(left)
                            .checked_pow(right)
                            .map(|value| Constant::BigInt {
                                negative: false,
                                digits: big_integer_digits(&value.to_string()),
                            })
                    }
                }
                (
                    Constant::Int(0) | Constant::Bool(false),
                    Operator::Pow,
                    Constant::BigInt {
                        negative: false, ..
                    },
                ) => Some(Constant::Int(0)),
                (Constant::Int(left), Operator::LShift, Constant::Int(right)) => {
                    u32::try_from(right)
                        .ok()
                        .and_then(|right| left.checked_shl(right))
                        .map(Constant::Int)
                }
                (Constant::Int(left), Operator::RShift, Constant::Int(right)) => {
                    u32::try_from(right)
                        .ok()
                        .and_then(|right| left.checked_shr(right))
                        .map(Constant::Int)
                }
                (
                    left @ (Constant::Int(_)
                    | Constant::BigInt {
                        negative: false, ..
                    }),
                    operator @ (Operator::BitAnd | Operator::BitOr | Operator::BitXor),
                    right @ (Constant::Int(_)
                    | Constant::BigInt {
                        negative: false, ..
                    }),
                ) => fold_positive_integer_bitwise(&left, operator, &right),
                (Constant::Float(left), Operator::Add, Constant::Float(right)) => {
                    Some(Constant::Float(left + right))
                }
                (Constant::Float(left), Operator::Sub, Constant::Float(right)) => {
                    Some(Constant::Float(left - right))
                }
                (Constant::Float(left), Operator::Mult, Constant::Float(right)) => {
                    Some(Constant::Float(left * right))
                }
                (Constant::Float(left), Operator::Div, Constant::Float(right)) if right != 0.0 => {
                    Some(Constant::Float(left / right))
                }
                (left, operator @ (Operator::Add | Operator::Sub | Operator::Div), right)
                    if matches!(left, Constant::Float(_))
                        && matches!(
                            right,
                            Constant::Bool(_) | Constant::Int(_) | Constant::SignedInt(_)
                        )
                        || matches!(right, Constant::Float(_))
                            && matches!(
                                left,
                                Constant::Bool(_) | Constant::Int(_) | Constant::SignedInt(_)
                            ) =>
                {
                    let to_float = |constant: Constant| match constant {
                        Constant::Bool(value) => Some(f64::from(value)),
                        Constant::Int(value) => Some(value as f64),
                        Constant::SignedInt(value) => Some(value as f64),
                        Constant::Float(value) => Some(value),
                        _ => None,
                    };
                    let left = to_float(left)?;
                    let right = to_float(right)?;
                    match operator {
                        Operator::Add => Some(Constant::Float(left + right)),
                        Operator::Sub => Some(Constant::Float(left - right)),
                        Operator::Div if right != 0.0 => Some(Constant::Float(left / right)),
                        Operator::Div => None,
                        _ => unreachable!(),
                    }
                }
                (Constant::Float(left), Operator::Pow, Constant::Float(right)) => {
                    let result = left.powf(right);
                    result.is_finite().then_some(Constant::Float(result))
                }
                (Constant::Float(left), Operator::FloorDiv, Constant::Int(right)) if right != 0 => {
                    Some(Constant::Float((left / right as f64).floor()))
                }
                (Constant::Bool(left), Operator::Pow, Constant::Bool(right)) => {
                    Some(Constant::Int(u64::from(left).pow(u32::from(right))))
                }
                (left, operator @ (Operator::Add | Operator::Sub | Operator::Mult), right)
                    if matches!(left, Constant::Complex { .. })
                        || matches!(right, Constant::Complex { .. }) =>
                {
                    let (left_real, left_imag) = constant_complex_parts(&left)?;
                    let (right_real, right_imag) = constant_complex_parts(&right)?;
                    let (real, imag) = match operator {
                        Operator::Add => (left_real + right_real, left_imag + right_imag),
                        Operator::Sub => (left_real - right_real, left_imag - right_imag),
                        Operator::Mult => (
                            left_real * right_real - left_imag * right_imag,
                            left_real * right_imag + left_imag * right_real,
                        ),
                        _ => unreachable!(),
                    };
                    Some(Constant::Complex { real, imag })
                }
                (Constant::String(mut left), Operator::Add, Constant::String(right)) => {
                    left.push_str(&right);
                    Some(Constant::String(left))
                }
                (Constant::Bytes(mut left), Operator::Add, Constant::Bytes(right)) => {
                    left.extend(right);
                    Some(Constant::Bytes(left))
                }
                (Constant::String(value), Operator::Mult, Constant::Int(times))
                | (Constant::Int(times), Operator::Mult, Constant::String(value)) => {
                    let length = value.chars().count();
                    let Some(max_times) = 4096_usize.checked_div(length) else {
                        return Some(Constant::String(String::new()));
                    };
                    let times = usize::try_from(times).ok()?;
                    (times <= max_times).then(|| Constant::String(value.repeat(times)))
                }
                (Constant::Bytes(value), Operator::Mult, Constant::Int(times))
                | (Constant::Int(times), Operator::Mult, Constant::Bytes(value)) => {
                    let Some(max_times) = 4096_usize.checked_div(value.len()) else {
                        return Some(Constant::Bytes(Vec::new()));
                    };
                    let times = usize::try_from(times).ok()?;
                    (times <= max_times).then(|| Constant::Bytes(value.repeat(times)))
                }
                (Constant::Tuple(value), Operator::Mult, Constant::Int(times))
                | (Constant::Int(times), Operator::Mult, Constant::Tuple(value)) => {
                    let Some(max_times) = 256_usize.checked_div(value.len()) else {
                        return Some(Constant::Tuple(Vec::new()));
                    };
                    let times = usize::try_from(times).ok()?;
                    (times <= max_times).then(|| {
                        Constant::Tuple((0..times).flat_map(|_| value.iter().cloned()).collect())
                    })
                }
                _ => None,
            }
        }
        _ => None,
    }
}

fn fold_positive_integer_bitwise(
    left: &Constant,
    operator: Operator,
    right: &Constant,
) -> Option<Constant> {
    let left = positive_integer_digits(left)?;
    let right = positive_integer_digits(right)?;
    let length = match operator {
        Operator::BitAnd => left.len().min(right.len()),
        Operator::BitOr | Operator::BitXor => left.len().max(right.len()),
        _ => unreachable!(),
    };
    let mut digits = Vec::with_capacity(length);
    for index in 0..length {
        let left = left.get(index).copied().unwrap_or(0);
        let right = right.get(index).copied().unwrap_or(0);
        digits.push(match operator {
            Operator::BitAnd => left & right,
            Operator::BitOr => left | right,
            Operator::BitXor => left ^ right,
            _ => unreachable!(),
        });
    }
    Some(positive_integer_constant(digits))
}

fn fold_positive_integer_add(left: &Constant, right: &Constant) -> Option<Constant> {
    let left = positive_integer_digits(left)?;
    let right = positive_integer_digits(right)?;
    let mut digits = Vec::with_capacity(left.len().max(right.len()) + 1);
    let mut carry = 0_u32;
    for index in 0..left.len().max(right.len()) {
        let value = u32::from(left.get(index).copied().unwrap_or(0))
            + u32::from(right.get(index).copied().unwrap_or(0))
            + carry;
        digits.push((value & 0x7fff) as u16);
        carry = value >> 15;
    }
    if carry != 0 {
        digits.push(u16::try_from(carry).expect("base-2^15 carry fits in u16"));
    }
    Some(positive_integer_constant(digits))
}

fn positive_integer_digits(value: &Constant) -> Option<Vec<u16>> {
    match value {
        Constant::Int(value) => {
            let mut value = *value;
            let mut digits = Vec::new();
            while value != 0 {
                digits.push((value & 0x7fff) as u16);
                value >>= 15;
            }
            Some(digits)
        }
        Constant::BigInt {
            negative: false,
            digits,
        } => Some(normalized_digits(digits.clone())),
        _ => None,
    }
}

fn positive_integer_constant(digits: Vec<u16>) -> Constant {
    let digits = normalized_digits(digits);
    if digits.len() <= 5 {
        let value = digits
            .iter()
            .enumerate()
            .fold(0_u128, |value, (index, digit)| {
                value | (u128::from(*digit) << (index * 15))
            });
        if let Ok(value) = u64::try_from(value) {
            return Constant::Int(value);
        }
    }
    Constant::BigInt {
        negative: false,
        digits,
    }
}

fn fold_literal_subscript(value: &Expr, index: &Expr) -> Option<Constant> {
    fn integer(expression: &Expr) -> Option<usize> {
        match fold_constant(expression)? {
            Constant::Bool(value) => Some(usize::from(value)),
            Constant::Int(value) => usize::try_from(value).ok(),
            Constant::SignedInt(value) if value >= 0 => usize::try_from(value).ok(),
            Constant::SignedInt(_) => None,
            Constant::BigInt {
                negative: false, ..
            } => Some(usize::MAX),
            Constant::BigInt { negative: true, .. } => None,
            _ => None,
        }
    }

    fn bound(expression: Option<&Expr>, length: usize, default: usize) -> Option<usize> {
        let Some(expression) = expression else {
            return Some(default);
        };
        integer(expression).map(|value| value.min(length))
    }

    fn indices(slice: &ruff_python_ast::ExprSlice, length: usize) -> Option<Vec<usize>> {
        let lower = bound(slice.lower.as_deref(), length, 0)?;
        let upper = bound(slice.upper.as_deref(), length, length)?;
        let step = slice.step.as_deref().map_or(Some(1), integer)?;
        if step == 0 {
            return None;
        }
        Some((lower.min(upper)..upper).step_by(step).collect())
    }

    let value = fold_constant(value)?;
    if !matches!(index, Expr::Slice(_)) {
        let index = match fold_constant(index)? {
            Constant::Bool(value) => i128::from(u8::from(value)),
            Constant::Int(value) => i128::from(value),
            Constant::SignedInt(value) => i128::from(value),
            _ => return None,
        };
        let length = match &value {
            Constant::String(value) => value.chars().count(),
            Constant::Bytes(value) => value.len(),
            Constant::Tuple(value) => value.len(),
            _ => return None,
        };
        let index = if index < 0 {
            i128::try_from(length).ok()?.checked_add(index)?
        } else {
            index
        };
        let index = usize::try_from(index)
            .ok()
            .filter(|index| *index < length)?;
        return match value {
            Constant::String(value) => value
                .chars()
                .nth(index)
                .map(|character| Constant::String(character.to_string())),
            Constant::Bytes(value) => Some(Constant::Int(u64::from(value[index]))),
            Constant::Tuple(value) => Some(value[index].clone()),
            _ => None,
        };
    }

    let Expr::Slice(slice) = index else {
        unreachable!();
    };
    if slice.step.is_some() && constant_slice(slice).is_none() {
        return None;
    }

    match value {
        Constant::String(value) => {
            let characters = value.chars().collect::<Vec<_>>();
            Some(Constant::String(
                indices(slice, characters.len())?
                    .into_iter()
                    .map(|index| characters[index])
                    .collect(),
            ))
        }
        Constant::Bytes(value) => Some(Constant::Bytes(
            indices(slice, value.len())?
                .into_iter()
                .map(|index| value[index])
                .collect(),
        )),
        Constant::Tuple(value) => Some(Constant::Tuple(
            indices(slice, value.len())?
                .into_iter()
                .map(|index| value[index].clone())
                .collect(),
        )),
        _ => None,
    }
}

pub(super) fn folded_bool_operand(expression: &Expr) -> Option<(&Expr, Constant)> {
    let Expr::BoolOp(boolean) = expression else {
        return fold_constant(expression).map(|constant| (expression, constant));
    };
    let (last, leading) = boolean.values.split_last()?;
    for value in leading {
        if matches!(value, Expr::BoolOp(_)) {
            return None;
        }
        let constant = fold_constant(value)?;
        let truthiness = constant_truthiness(&constant);
        if matches!(boolean.op, BoolOp::And) && !truthiness
            || matches!(boolean.op, BoolOp::Or) && truthiness
        {
            return Some((value, constant));
        }
    }
    if matches!(last, Expr::BoolOp(_)) {
        None
    } else {
        fold_constant(last).map(|constant| (last, constant))
    }
}

#[expect(
    clippy::cast_precision_loss,
    reason = "CPython complex folding intentionally rounds integer operands to binary64"
)]
fn constant_complex_parts(constant: &Constant) -> Option<(f64, f64)> {
    match constant {
        Constant::Int(value) => Some((*value as f64, 0.0)),
        Constant::SignedInt(value) => Some((*value as f64, 0.0)),
        Constant::Float(value) => Some((*value, 0.0)),
        Constant::Complex { real, imag } => Some((*real, *imag)),
        _ => None,
    }
}

pub(super) fn constant_truthiness(constant: &Constant) -> bool {
    match constant {
        Constant::None => false,
        Constant::Bool(value) => *value,
        Constant::Ellipsis | Constant::Slice { .. } | Constant::Code(_) => true,
        Constant::Int(value) => *value != 0,
        Constant::SignedInt(value) => *value != 0,
        Constant::BigInt { digits, .. } => !digits.is_empty(),
        Constant::Float(value) => *value != 0.0,
        Constant::Complex { real, imag } => *real != 0.0 || *imag != 0.0,
        Constant::String(value) => !value.is_empty(),
        Constant::SurrogateString(value) => !value.is_empty(),
        Constant::Bytes(value) => !value.is_empty(),
        Constant::Tuple(value) | Constant::FrozenSet(value) => !value.is_empty(),
    }
}

pub(super) fn constant_slice(slice: &ruff_python_ast::ExprSlice) -> Option<Constant> {
    fn bound(expression: Option<&Expr>) -> Option<Constant> {
        expression.map_or(Some(Constant::None), literal_constant)
    }

    Some(Constant::Slice {
        lower: Box::new(bound(slice.lower.as_deref())?),
        upper: Box::new(bound(slice.upper.as_deref())?),
        step: Box::new(bound(slice.step.as_deref())?),
    })
}

pub(super) fn two_element_slice_optimization(slice: &ruff_python_ast::ExprSlice) -> bool {
    slice.step.is_none() && constant_slice(slice).is_none()
}

pub(super) fn first_literal_constant(expression: &Expr) -> Option<Constant> {
    if let Some(constant) = literal_constant(expression) {
        return Some(constant);
    }
    match expression {
        Expr::BinOp(binary) => {
            first_literal_constant(&binary.left).or_else(|| first_literal_constant(&binary.right))
        }
        Expr::UnaryOp(unary) => first_literal_constant(&unary.operand),
        Expr::Tuple(tuple) => tuple.elts.iter().find_map(first_literal_constant),
        _ => None,
    }
}

pub(super) fn first_suite_literal_constant(body: &[Stmt]) -> Option<Constant> {
    #[derive(Default)]
    struct Collector {
        constant: Option<Constant>,
    }

    impl<'ast> Visitor<'ast> for Collector {
        fn visit_stmt(&mut self, statement: &'ast Stmt) {
            if self.constant.is_some()
                || matches!(statement, Stmt::FunctionDef(_) | Stmt::ClassDef(_))
            {
                return;
            }
            walk_stmt(self, statement);
        }

        fn visit_expr(&mut self, expression: &'ast Expr) {
            if self.constant.is_some() || matches!(expression, Expr::Lambda(_)) {
                return;
            }
            if let Some(constant) = literal_constant(expression) {
                self.constant = Some(constant);
            } else {
                walk_expr(self, expression);
            }
        }
    }

    let mut collector = Collector::default();
    for statement in body {
        collector.visit_stmt(statement);
        if collector.constant.is_some() {
            break;
        }
    }
    collector.constant
}
pub(super) fn is_literal_constant(expression: &Expr) -> bool {
    matches!(
        expression,
        Expr::NoneLiteral(_)
            | Expr::BooleanLiteral(_)
            | Expr::EllipsisLiteral(_)
            | Expr::NumberLiteral(_)
            | Expr::StringLiteral(_)
            | Expr::BytesLiteral(_)
    )
}
pub(super) fn big_integer_digits(token: &str) -> Vec<u16> {
    let token: String = token
        .chars()
        .filter(|character| *character != '_')
        .collect();
    let (digits, radix) = if let Some(digits) = token
        .strip_prefix("0x")
        .or_else(|| token.strip_prefix("0X"))
    {
        (digits, 16)
    } else if let Some(digits) = token
        .strip_prefix("0o")
        .or_else(|| token.strip_prefix("0O"))
    {
        (digits, 8)
    } else if let Some(digits) = token
        .strip_prefix("0b")
        .or_else(|| token.strip_prefix("0B"))
    {
        (digits, 2)
    } else {
        (token.as_str(), 10)
    };

    let mut value = vec![0_u16];
    for digit in digits.chars().filter_map(|digit| digit.to_digit(radix)) {
        let mut carry = digit;
        for limb in &mut value {
            let next = u32::from(*limb) * radix + carry;
            *limb = (next & 0x7fff) as u16;
            carry = next >> 15;
        }
        while carry != 0 {
            value.push((carry & 0x7fff) as u16);
            carry >>= 15;
        }
    }
    while value.len() > 1 && value.last() == Some(&0) {
        value.pop();
    }
    value
}
