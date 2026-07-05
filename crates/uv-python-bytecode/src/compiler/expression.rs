// Compiler submodules intentionally share the parent-owned state and lowering vocabulary.
#![allow(clippy::wildcard_imports)]

use super::*;

impl Compiler {
    pub(super) fn compile_match(
        &mut self,
        statement: &ruff_python_ast::StmtMatch,
    ) -> Result<(), CompileError> {
        self.compile_match_inner(statement, false)
    }

    pub(super) fn compile_match_inner(
        &mut self,
        statement: &ruff_python_ast::StmtMatch,
        terminal: bool,
    ) -> Result<(), CompileError> {
        let base_depth = self.control.depth;
        if matches!(statement.subject.as_ref(), Expr::Tuple(_))
            && let Some(constant) = fold_constant(&statement.subject)
        {
            if self.output.constants.is_empty()
                && let Some(seed) = first_literal_constant(&statement.subject)
            {
                self.add_constant(seed)?;
            }
            self.assembler
                .set_location(self.source_location(statement.subject.range()));
            self.emit_deferred_constant(constant)?;
        } else {
            self.compile_expression(&statement.subject)?;
        }
        let end = self.assembler.label();
        let has_default = statement.cases.len() > 1
            && statement
                .cases
                .last()
                .is_some_and(|case| is_wildcard_pattern(&case.pattern));
        let matched_cases = statement.cases.len() - usize::from(has_default);
        let mut failure_reaches_end = false;

        for (index, case) in statement.cases[..matched_cases].iter().enumerate() {
            let copied_subject = index + 1 != matched_cases;
            if copied_subject {
                self.assembler
                    .set_location(self.source_location(case.pattern.range()));
                self.emit(COPY, 1, 1)?;
            }
            let failure_depth = base_depth + i32::from(copied_subject);
            let mut context = MatchContext {
                stores: Vec::new(),
                fail_pop: Vec::new(),
                on_top: 0,
            };
            self.compile_stack_pattern(&case.pattern, &mut context)?;
            for name in &context.stores {
                self.assembler
                    .set_location(self.source_location(case.pattern.range()));
                self.store_name(name)?;
            }
            let mut guard_always_false = false;
            let mut irrefutable_true_guard = false;
            if let Some(guard) = &case.guard {
                if let Some(truthiness) = early_condition_truthiness(guard) {
                    self.record_folded_value(guard)?;
                    irrefutable_true_guard = truthiness && context.fail_pop.is_empty();
                    if irrefutable_true_guard {
                        let location = self.source_location(guard.range());
                        if self
                            .assembler
                            .last_instruction_location()
                            .is_none_or(|previous| previous.line != location.line)
                        {
                            self.assembler.set_location(location);
                            self.emit(NOP, 0, 0)?;
                        }
                    }
                    if !truthiness || !context.fail_pop.is_empty() {
                        let range = if is_wildcard_pattern(&case.pattern) {
                            case.pattern.range()
                        } else {
                            guard.range()
                        };
                        self.assembler.set_location(self.source_location(range));
                        self.emit(NOP, 0, 0)?;
                    }
                    if !truthiness {
                        self.ensure_match_fail_pop(&mut context, 0);
                        guard_always_false = true;
                    }
                } else {
                    self.ensure_match_fail_pop(&mut context, 0);
                    self.compile_jump_if(guard, false, context.fail_pop[0])?;
                }
            }
            if !guard_always_false {
                if copied_subject {
                    self.assembler.set_location(
                        case.body.first().map_or(SourceLocation::NONE, |statement| {
                            self.source_location(statement_execution_range(statement))
                        }),
                    );
                    self.emit(POP_TOP, 0, -1)?;
                }
                let body_previous_location = self.assembler.last_instruction_location();
                let body_start = self.assembler.instruction_count();
                let tail_if = if !terminal
                    && let Some((Stmt::If(statement), leading)) = case.body.split_last()
                    && statement.elif_else_clauses.is_empty()
                    && early_condition_truthiness(&statement.test).is_none()
                    && !suite_terminates(&statement.body)
                {
                    Some((statement, leading))
                } else {
                    None
                };
                let emitted_case_exit = if let Some((statement, leading)) = tail_if {
                    self.compile_suite(leading)?;
                    self.compile_match_case_tail_if(statement, end)?;
                    true
                } else {
                    false
                };
                let emitted_fallthrough = if emitted_case_exit {
                    false
                } else if terminal && matches!(case.body.last(), Some(Stmt::If(_))) {
                    let ((), emitted_fallthrough) = self.with_fallthrough_tracking(|compiler| {
                        compiler.compile_suite_inner(&case.body, true)
                    })?;
                    emitted_fallthrough
                } else {
                    self.compile_suite(&case.body)?;
                    false
                };
                if !emitted_case_exit && !suite_terminates(&case.body) && !emitted_fallthrough {
                    self.set_branch_end_location(&case.body, body_start);
                    if terminal {
                        self.emit_deferred_implicit_return()?;
                    } else {
                        self.emit_jump_forward(JUMP_FORWARD, end, 0)?;
                        if index + 1 == matched_cases && !context.fail_pop.is_empty() {
                            // CPython duplicates small exits before removing this jump across
                            // the empty final pattern-failure block.
                            self.assembler.defer_last_jump_removal();
                        }
                        let subject_pop_is_optimized_away = !copied_subject
                            && context.stores.is_empty()
                            && fold_constant(&statement.subject).is_some();
                        let previous_instruction_covers_branch = context.fail_pop.is_empty()
                            && !subject_pop_is_optimized_away
                            && body_previous_location.is_some_and(|location| {
                                location.line == self.assembler.location().line
                            });
                        if irrefutable_true_guard
                            || (self.assembler.instruction_count() == body_start + 1
                                && !previous_instruction_covers_branch
                                && !matches!(self.unit.scope, Scope::Class { .. }))
                        {
                            self.assembler.preserve_last_inlined_jump_nop();
                        }
                    }
                }
            } else {
                self.pre_register_suite_names(&case.body)?;
            }
            if !context.fail_pop.is_empty() {
                self.emit_match_fail_pop(
                    &context,
                    self.source_location(case.pattern.range()),
                    failure_depth,
                )?;
                if index + 1 != matched_cases {
                    // CPython starts every following case in a fresh CFG block. Keep the
                    // failure cleanup separate so it cannot absorb the next case as a small
                    // exit when a large OR pattern pushes its success target out of range.
                    let boundary = self.assembler.label();
                    self.assembler.preserve_block_boundary(boundary);
                    self.assembler.mark(boundary);
                }
                failure_reaches_end = true;
            } else {
                failure_reaches_end = false;
                if index + 1 != matched_cases {
                    // A constant-true guard can make a syntactically guarded,
                    // irrefutable case unconditional. The remaining cases are
                    // unreachable, but code generation still starts them with
                    // the original subject on the stack.
                    self.set_depth(failure_depth);
                }
            }
        }

        if has_default {
            let case = statement.cases.last().unwrap();
            self.assembler
                .set_location(self.source_location(case.pattern.range()));
            self.emit(NOP, 0, 0)?;
            if let Some(guard) = &case.guard {
                self.compile_jump_if(guard, false, end)?;
                failure_reaches_end = true;
            }
            let body_start = self.assembler.instruction_count();
            let emitted_fallthrough = if terminal && matches!(case.body.last(), Some(Stmt::If(_))) {
                let ((), emitted_fallthrough) = self.with_fallthrough_tracking(|compiler| {
                    compiler.compile_suite_inner(&case.body, true)
                })?;
                emitted_fallthrough
            } else {
                self.compile_suite(&case.body)?;
                false
            };
            if terminal && !suite_terminates(&case.body) && !emitted_fallthrough {
                self.set_branch_end_location(&case.body, body_start);
                self.emit_deferred_implicit_return()?;
                failure_reaches_end = case.guard.is_some();
            }
        }

        self.assembler.mark(end);
        self.set_depth(base_depth);
        if terminal && failure_reaches_end {
            self.assembler.set_location(SourceLocation::NONE);
            self.emit_deferred_implicit_return()?;
        }
        self.set_depth(base_depth);
        Ok(())
    }

    fn compile_match_case_tail_if(
        &mut self,
        statement: &ruff_python_ast::StmtIf,
        match_end: Label,
    ) -> Result<(), CompileError> {
        let base_depth = self.control.depth;
        let false_exit = self.assembler.label();
        self.compile_jump_if(&statement.test, false, false_exit)?;
        self.mark_definitely_evaluated_locals(&statement.test);

        let body_start = self.assembler.instruction_count();
        self.compile_suite(&statement.body)?;
        self.set_branch_end_location(&statement.body, body_start);
        self.emit_jump_forward(JUMP_FORWARD, match_end, 0)?;

        self.assembler.mark(false_exit);
        self.set_depth(base_depth);
        self.assembler
            .set_location(self.source_location(statement.test.range()));
        self.emit_jump_forward(JUMP_FORWARD, match_end, 0)?;
        // CPython leaves the condition's false edge pointing at this case-exit block, even
        // though the block itself immediately jumps to the match join.
        self.assembler.prevent_last_jump_threading_target();
        Ok(())
    }

    fn ensure_match_fail_pop(&mut self, context: &mut MatchContext, pops: usize) {
        while context.fail_pop.len() <= pops {
            let label = self.assembler.label();
            context.fail_pop.push(label);
        }
    }

    fn jump_to_match_fail(
        &mut self,
        context: &mut MatchContext,
        opcode: Opcode,
        effect: i32,
    ) -> Result<(), CompileError> {
        let pops = context.on_top + context.stores.len();
        self.ensure_match_fail_pop(context, pops);
        self.emit_jump_forward(opcode, context.fail_pop[pops], effect)?;
        if matches!(
            opcode,
            POP_JUMP_IF_FALSE | POP_JUMP_IF_NONE | POP_JUMP_IF_NOT_NONE | POP_JUMP_IF_TRUE
        ) {
            self.emit(NOT_TAKEN, 0, 0)?;
        }
        Ok(())
    }

    fn emit_match_fail_pop(
        &mut self,
        context: &MatchContext,
        location: SourceLocation,
        failure_depth: i32,
    ) -> Result<(), CompileError> {
        for pops in (1..context.fail_pop.len()).rev() {
            self.assembler.mark(context.fail_pop[pops]);
            self.set_depth(failure_depth + i32::try_from(pops).unwrap());
            self.assembler.set_location(location);
            self.emit(POP_TOP, 0, -1)?;
        }
        self.assembler.mark(context.fail_pop[0]);
        self.set_depth(failure_depth);
        Ok(())
    }

    fn compile_stack_pattern(
        &mut self,
        pattern: &Pattern,
        context: &mut MatchContext,
    ) -> Result<(), CompileError> {
        self.assembler
            .set_location(self.source_location(pattern.range()));
        match pattern {
            Pattern::MatchValue(pattern) => {
                if let Some(constant) = fold_constant(&pattern.value) {
                    self.emit_preprocessed_constant(&pattern.value, constant)?;
                } else {
                    self.compile_expression(&pattern.value)?;
                }
                self.assembler
                    .set_location(self.source_location(pattern.range));
                self.emit_comparison(ComparisonOperation::EQ, true)?;
                self.jump_to_match_fail(context, POP_JUMP_IF_FALSE, -1)
            }
            Pattern::MatchSingleton(pattern) => {
                let constant = self.add_constant(match pattern.value {
                    Singleton::None => Constant::None,
                    Singleton::True => Constant::Bool(true),
                    Singleton::False => Constant::Bool(false),
                })?;
                self.emit(LOAD_CONST, constant, 1)?;
                self.emit(IS_OP, 0, -1)?;
                self.jump_to_match_fail(context, POP_JUMP_IF_FALSE, -1)
            }
            Pattern::MatchAs(pattern) => {
                if let Some(inner) = &pattern.pattern {
                    if !is_wildcard_pattern(inner) {
                        context.on_top += 1;
                        self.emit(COPY, 1, 1)?;
                        self.compile_stack_pattern(inner, context)?;
                        context.on_top -= 1;
                    }
                }
                self.assembler
                    .set_location(self.source_location(pattern.range));
                self.capture_match_name(pattern.name.as_ref().map(Identifier::as_str), context)
            }
            Pattern::MatchStar(pattern) => {
                self.capture_match_name(pattern.name.as_ref().map(Identifier::as_str), context)
            }
            Pattern::MatchOr(pattern) => self.compile_stack_or_pattern(pattern, context),
            Pattern::MatchSequence(pattern) => {
                self.compile_stack_sequence_pattern(pattern, context)
            }
            Pattern::MatchMapping(pattern) => self.compile_stack_mapping_pattern(pattern, context),
            Pattern::MatchClass(pattern) => self.compile_stack_class_pattern(pattern, context),
        }
    }

    fn capture_match_name(
        &mut self,
        name: Option<&str>,
        context: &mut MatchContext,
    ) -> Result<(), CompileError> {
        let Some(name) = name else {
            return self.emit(POP_TOP, 0, -1);
        };
        let mut rotations = context.on_top + context.stores.len() + 1;
        while rotations > 1 {
            self.emit(SWAP, to_u32(rotations, "pattern capture rotation")?, 0)?;
            rotations -= 1;
        }
        context.stores.push(name.to_string());
        Ok(())
    }

    fn rotate_match_stack(&mut self, mut count: usize) -> Result<(), CompileError> {
        while count > 1 {
            self.emit(SWAP, to_u32(count, "pattern capture rotation")?, 0)?;
            count -= 1;
        }
        Ok(())
    }

    fn compile_stack_or_pattern(
        &mut self,
        pattern: &ruff_python_ast::PatternMatchOr,
        context: &mut MatchContext,
    ) -> Result<(), CompileError> {
        let entry_depth = self.control.depth;
        let end = self.assembler.label();
        let mut control: Option<Vec<String>> = None;
        for (index, alternative) in pattern.patterns.iter().enumerate() {
            self.assembler
                .set_location(self.source_location(alternative.range()));
            self.emit(COPY, 1, 1)?;
            let mut alternative_context = MatchContext {
                stores: Vec::new(),
                fail_pop: Vec::new(),
                on_top: 0,
            };
            self.compile_stack_pattern(alternative, &mut alternative_context)?;

            if index != 0 {
                let control = control.as_ref().unwrap();
                if alternative_context.stores.len() != control.len() {
                    return Err(CompileError::Parse(
                        "alternative patterns bind different names".to_string(),
                    ));
                }
                for control_index in (0..control.len()).rev() {
                    let Some(store_index) = alternative_context
                        .stores
                        .iter()
                        .position(|name| name == &control[control_index])
                    else {
                        return Err(CompileError::Parse(
                            "alternative patterns bind different names".to_string(),
                        ));
                    };
                    if control_index != store_index {
                        debug_assert!(store_index < control_index);
                        let rotations = store_index + 1;
                        let rotated: Vec<_> =
                            alternative_context.stores.drain(..rotations).collect();
                        alternative_context.stores.splice(
                            control_index - store_index..control_index - store_index,
                            rotated,
                        );
                        for _ in 0..rotations {
                            self.rotate_match_stack(control_index + 1)?;
                        }
                    }
                }
            }

            self.assembler
                .set_location(self.source_location(alternative.range()));
            self.emit_jump_forward(JUMP_FORWARD, end, 0)?;
            if alternative_context.fail_pop.is_empty() {
                self.set_depth(entry_depth);
            } else {
                self.emit_match_fail_pop(
                    &alternative_context,
                    self.source_location(alternative.range()),
                    entry_depth,
                )?;
            }
            if index == 0 {
                control = Some(alternative_context.stores);
            }
        }
        self.assembler
            .set_location(self.source_location(pattern.range));
        self.emit(POP_TOP, 0, -1)?;
        self.jump_to_match_fail(context, JUMP_FORWARD, 0)?;
        self.assembler.mark(end);
        let control = control.unwrap();
        self.set_depth(entry_depth + i32::try_from(control.len()).unwrap());
        self.assembler
            .set_location(self.source_location(pattern.range));
        let rotations = control.len() + 1 + context.on_top + context.stores.len();
        for name in control {
            self.rotate_match_stack(rotations)?;
            if context.stores.contains(&name) {
                return Err(CompileError::Parse(format!(
                    "multiple assignments to name '{name}' in pattern"
                )));
            }
            context.stores.push(name);
        }
        self.emit(POP_TOP, 0, -1)
    }

    fn compile_stack_sequence_pattern(
        &mut self,
        pattern: &ruff_python_ast::PatternMatchSequence,
        context: &mut MatchContext,
    ) -> Result<(), CompileError> {
        let starred = pattern
            .patterns
            .iter()
            .position(|pattern| matches!(pattern, Pattern::MatchStar(_)));
        let minimum = pattern.patterns.len() - usize::from(starred.is_some());
        let only_wildcards = pattern.patterns.iter().all(|pattern| {
            matches!(pattern, Pattern::MatchAs(pattern) if pattern.pattern.is_none() && pattern.name.is_none())
                || matches!(pattern, Pattern::MatchStar(pattern) if pattern.name.is_none())
        });
        context.on_top += 1;
        self.emit(MATCH_SEQUENCE, 0, 1)?;
        self.jump_to_match_fail(context, POP_JUMP_IF_FALSE, -1)?;
        if starred.is_none() || pattern.patterns.len() > 1 {
            self.emit(GET_LEN, 0, 1)?;
            if self.output.constants.is_empty() {
                self.add_constant(Constant::Int(u64::try_from(minimum).unwrap_or(u64::MAX)))?;
            }
            self.emit(
                LOAD_SMALL_INT,
                to_u32(minimum, "sequence pattern length")?,
                1,
            )?;
            self.emit_comparison(
                if starred.is_some() {
                    ComparisonOperation::GE
                } else {
                    ComparisonOperation::EQ
                },
                false,
            )?;
            self.jump_to_match_fail(context, POP_JUMP_IF_FALSE, -1)?;
        }
        context.on_top -= 1;
        if only_wildcards {
            return self.emit(POP_TOP, 0, -1);
        }
        if let Some(starred) = starred
            && matches!(&pattern.patterns[starred], Pattern::MatchStar(pattern) if pattern.name.is_none())
        {
            context.on_top += 1;
            for (index, subpattern) in pattern.patterns.iter().enumerate() {
                if index == starred || is_wildcard_pattern(subpattern) {
                    continue;
                }
                self.assembler
                    .set_location(self.source_location(pattern.range));
                self.emit(COPY, 1, 1)?;
                if index < starred {
                    self.emit(LOAD_SMALL_INT, to_u32(index, "sequence pattern index")?, 1)?;
                } else {
                    self.emit(GET_LEN, 0, 1)?;
                    self.emit(
                        LOAD_SMALL_INT,
                        to_u32(
                            pattern.patterns.len() - index,
                            "sequence pattern suffix index",
                        )?,
                        1,
                    )?;
                    self.emit_binary_operation(binary_operator(Operator::Sub, false))?;
                }
                self.emit_binary_operation(BinaryOperation::SUBSCR)?;
                self.compile_stack_pattern(subpattern, context)?;
            }
            context.on_top -= 1;
            self.assembler
                .set_location(self.source_location(pattern.range));
            return self.emit(POP_TOP, 0, -1);
        }
        if let Some(starred) = starred {
            let before = to_u32(starred, "sequence pattern prefix length")?;
            let after = to_u32(
                pattern.patterns.len() - starred - 1,
                "sequence pattern suffix length",
            )?;
            self.emit(
                UNPACK_EX,
                before | (after << 8),
                i32::try_from(pattern.patterns.len()).unwrap() - 1,
            )?;
        } else {
            self.emit(
                UNPACK_SEQUENCE,
                to_u32(pattern.patterns.len(), "sequence pattern length")?,
                i32::try_from(pattern.patterns.len()).unwrap() - 1,
            )?;
        }
        context.on_top += pattern.patterns.len();
        for subpattern in &pattern.patterns {
            context.on_top -= 1;
            self.compile_stack_pattern(subpattern, context)?;
        }
        Ok(())
    }

    fn compile_stack_mapping_pattern(
        &mut self,
        pattern: &ruff_python_ast::PatternMatchMapping,
        context: &mut MatchContext,
    ) -> Result<(), CompileError> {
        context.on_top += 1;
        self.emit(MATCH_MAPPING, 0, 1)?;
        self.jump_to_match_fail(context, POP_JUMP_IF_FALSE, -1)?;
        if pattern.keys.is_empty() && pattern.rest.is_none() {
            context.on_top -= 1;
            return self.emit(POP_TOP, 0, -1);
        }
        if !pattern.keys.is_empty() {
            self.emit(GET_LEN, 0, 1)?;
            self.add_constant(Constant::Int(
                u64::try_from(pattern.keys.len()).unwrap_or(u64::MAX),
            ))?;
            self.emit(
                LOAD_SMALL_INT,
                to_u32(pattern.keys.len(), "mapping pattern key count")?,
                1,
            )?;
            self.emit_comparison(ComparisonOperation::GE, false)?;
            self.jump_to_match_fail(context, POP_JUMP_IF_FALSE, -1)?;
        }
        if let Some(keys) = pattern
            .keys
            .iter()
            .map(fold_constant)
            .collect::<Option<Vec<_>>>()
        {
            self.emit_deferred_constant(Constant::Tuple(keys))?;
        } else {
            for key in &pattern.keys {
                self.compile_expression(key)?;
            }
            self.emit_build(BUILD_TUPLE, pattern.keys.len())?;
        }
        self.emit(MATCH_KEYS, 0, 1)?;
        context.on_top += 2;
        self.emit(COPY, 1, 1)?;
        self.add_constant(Constant::None)?;
        self.jump_to_match_fail(context, POP_JUMP_IF_NONE, -1)?;
        self.emit(
            UNPACK_SEQUENCE,
            to_u32(pattern.patterns.len(), "mapping pattern value count")?,
            i32::try_from(pattern.patterns.len()).unwrap() - 1,
        )?;
        context.on_top += pattern.patterns.len();
        context.on_top -= 1;
        for subpattern in &pattern.patterns {
            context.on_top -= 1;
            self.compile_stack_pattern(subpattern, context)?;
        }
        context.on_top -= 2;
        if let Some(rest) = &pattern.rest {
            self.assembler
                .set_location(self.source_location(pattern.range));
            self.emit(BUILD_MAP, 0, 1)?;
            self.emit(SWAP, 3, 0)?;
            self.emit(DICT_UPDATE, 2, -1)?;
            self.emit(
                UNPACK_SEQUENCE,
                to_u32(pattern.keys.len(), "mapping pattern key count")?,
                i32::try_from(pattern.keys.len()).unwrap() - 1,
            )?;
            let mut size = pattern.keys.len();
            while size > 0 {
                self.emit(COPY, to_u32(1 + size, "mapping rest copy depth")?, 1)?;
                self.emit(SWAP, 2, 0)?;
                self.emit(DELETE_SUBSCR, 0, -2)?;
                size -= 1;
            }
            self.capture_match_name(Some(rest.as_str()), context)
        } else {
            self.assembler
                .set_location(self.source_location(pattern.range));
            self.emit(POP_TOP, 0, -1)?;
            self.emit(POP_TOP, 0, -1)
        }
    }

    fn compile_stack_class_pattern(
        &mut self,
        pattern: &ruff_python_ast::PatternMatchClass,
        context: &mut MatchContext,
    ) -> Result<(), CompileError> {
        self.compile_expression(&pattern.cls)?;
        let keyword_names = pattern
            .arguments
            .keywords
            .iter()
            .map(|keyword| Constant::String(keyword.attr.as_str().to_string()))
            .collect();
        let keyword_names = self.add_interned_string_tuple(keyword_names)?;
        self.emit(LOAD_CONST, keyword_names, 1)?;
        self.emit(
            MATCH_CLASS,
            to_u32(
                pattern.arguments.patterns.len(),
                "class positional pattern count",
            )?,
            -2,
        )?;
        self.emit(COPY, 1, 1)?;
        self.add_constant(Constant::None)?;
        context.on_top += 1;
        self.jump_to_match_fail(context, POP_JUMP_IF_NONE, -1)?;
        let subpatterns = pattern
            .arguments
            .patterns
            .iter()
            .chain(
                pattern
                    .arguments
                    .keywords
                    .iter()
                    .map(|keyword| &keyword.pattern),
            )
            .collect::<Vec<_>>();
        self.emit(
            UNPACK_SEQUENCE,
            to_u32(subpatterns.len(), "class pattern count")?,
            i32::try_from(subpatterns.len()).unwrap() - 1,
        )?;
        context.on_top += subpatterns.len();
        context.on_top -= 1;
        for subpattern in subpatterns {
            context.on_top -= 1;
            self.compile_stack_pattern(subpattern, context)?;
        }
        Ok(())
    }

    pub(super) fn compile_expression(&mut self, expression: &Expr) -> Result<(), CompileError> {
        let previous = self.assembler.location();
        self.assembler
            .set_location(self.source_location(expression.range()));
        let result = self.compile_expression_inner(expression);
        self.assembler.set_location(previous);
        result
    }

    fn compile_expression_inner(&mut self, expression: &Expr) -> Result<(), CompileError> {
        let starting_depth = self.control.depth;
        if matches!(expression, Expr::BoolOp(_))
            && let Some((origin, constant)) = folded_bool_operand(expression)
        {
            self.pre_register_expression_names(expression)?;
            let expression_location = self.source_location(expression.range());
            let origin_location = self.source_location(origin.range());
            if expression_location.line != origin_location.line {
                self.assembler.set_location(expression_location);
                self.emit(NOP, 0, 0)?;
            }
            self.assembler.set_location(origin_location);
            self.emit_folded_constant(origin, constant)?;
            return Ok(());
        }
        if matches!(
            expression,
            Expr::BinOp(_) | Expr::Name(_) | Expr::Subscript(_) | Expr::UnaryOp(_) | Expr::Tuple(_)
        ) && let Some(constant) = fold_constant(expression)
        {
            self.emit_folded_constant(expression, constant)?;
            return Ok(());
        }
        match expression {
            Expr::Name(name) if name.ctx == ExprContext::Load => {
                self.load_name(name.id.as_str())?;
            }
            Expr::NoneLiteral(_) => {
                let index = self.add_constant(Constant::None)?;
                self.emit(LOAD_CONST, index, 1)?;
            }
            Expr::BooleanLiteral(boolean) => {
                let index = self.add_constant(Constant::Bool(boolean.value))?;
                self.emit(LOAD_CONST, index, 1)?;
            }
            Expr::EllipsisLiteral(_) => {
                let index = self.add_constant(Constant::Ellipsis)?;
                self.emit(LOAD_CONST, index, 1)?;
            }
            Expr::NumberLiteral(number) => {
                let constant = match &number.value {
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
                };
                if let Number::Int(value) = &number.value
                    && let Some(value) = value.as_u8()
                {
                    if self.output.constants.is_empty() {
                        self.add_constant(constant)?;
                    }
                    self.emit(LOAD_SMALL_INT, u32::from(value), 1)?;
                } else {
                    let index = self.add_constant(constant)?;
                    self.emit(LOAD_CONST, index, 1)?;
                }
            }
            Expr::StringLiteral(string) => {
                let index = self.add_constant(self.string_literal_constant(string))?;
                self.emit(LOAD_CONST, index, 1)?;
            }
            Expr::BytesLiteral(bytes) => {
                let index = self.add_constant(Constant::Bytes(bytes.value.bytes().collect()))?;
                self.emit(LOAD_CONST, index, 1)?;
            }
            Expr::BinOp(binary) => {
                if !self.compile_optimized_percent_format(binary)? {
                    self.compile_expression(&binary.left)?;
                    self.compile_expression(&binary.right)?;
                    self.emit_binary_operation(binary_operator(binary.op, false))?;
                }
            }
            Expr::UnaryOp(unary) => {
                self.compile_expression(&unary.operand)?;
                match unary.op {
                    UnaryOp::Invert => self.emit(UNARY_INVERT, 0, 0)?,
                    UnaryOp::Not => {
                        self.emit(TO_BOOL, 0, 0)?;
                        self.emit(UNARY_NOT, 0, 0)?;
                    }
                    UnaryOp::UAdd => self.emit_unary_intrinsic(UnaryIntrinsic::UNARY_POSITIVE)?,
                    UnaryOp::USub => self.emit(UNARY_NEGATIVE, 0, 0)?,
                }
            }
            Expr::Lambda(lambda) => self.compile_lambda(lambda)?,
            Expr::Call(call) => {
                self.compile_call(call)?;
            }
            Expr::Attribute(attribute) if attribute.ctx == ExprContext::Load => {
                if !self.compile_super_attribute(attribute, false)? {
                    self.compile_expression(&attribute.value)?;
                    let index = self.name_index(attribute.attr.as_str())?;
                    self.assembler
                        .set_location(self.source_location(self.attribute_opcode_range(attribute)));
                    self.emit(LOAD_ATTR, index << 1, 0)?;
                }
            }
            Expr::List(list) if list.ctx == ExprContext::Load => {
                self.compile_collection(&list.elts, CollectionKind::List)?;
            }
            Expr::Tuple(tuple) if tuple.ctx == ExprContext::Load => {
                self.compile_collection(&tuple.elts, CollectionKind::Tuple)?;
            }
            Expr::Set(set) => {
                self.compile_collection(&set.elts, CollectionKind::Set)?;
            }
            Expr::Dict(dict) => {
                self.compile_dict(&dict.items)?;
            }
            Expr::Subscript(subscript) if subscript.ctx == ExprContext::Load => {
                self.compile_expression(&subscript.value)?;
                if let Expr::Slice(slice) = subscript.slice.as_ref() {
                    let slice_location = self.source_location(slice.range);
                    let subscript_location = self.source_location(subscript.range);
                    if let Some(constant) = constant_slice(slice) {
                        let index = self.add_constant(constant)?;
                        self.assembler.set_location(slice_location);
                        self.emit(LOAD_CONST, index, 1)?;
                        self.assembler.set_location(subscript_location);
                        self.emit_binary_operation(BinaryOperation::SUBSCR)?;
                    } else if slice.step.is_none() {
                        self.compile_optional_slice_bound(slice.lower.as_deref(), slice_location)?;
                        self.compile_optional_slice_bound(slice.upper.as_deref(), slice_location)?;
                        self.assembler.set_location(subscript_location);
                        self.emit(BINARY_SLICE, 0, -2)?;
                    } else {
                        self.compile_expression(&subscript.slice)?;
                        self.emit_binary_operation(BinaryOperation::SUBSCR)?;
                    }
                } else {
                    self.compile_expression(&subscript.slice)?;
                    self.emit_binary_operation(BinaryOperation::SUBSCR)?;
                }
            }
            Expr::If(expression) => self.compile_if_expression(expression)?,
            Expr::BoolOp(expression) => self.compile_bool_expression(expression)?,
            Expr::Compare(expression) => self.compile_compare_expression(expression)?,
            Expr::Named(expression) => {
                self.compile_expression(&expression.value)?;
                self.emit(COPY, 1, 1)?;
                self.compile_store_target(&expression.target)?;
            }
            Expr::Slice(slice) => self.compile_slice(slice)?,
            Expr::FString(fstring) => self.compile_fstring(fstring)?,
            Expr::ListComp(comprehension) => self.compile_comprehension(
                &comprehension.generators,
                None,
                &comprehension.elt,
                BUILD_LIST,
                LIST_APPEND,
                false,
            )?,
            Expr::SetComp(comprehension) => self.compile_comprehension(
                &comprehension.generators,
                None,
                &comprehension.elt,
                BUILD_SET,
                SET_ADD,
                false,
            )?,
            Expr::DictComp(comprehension) => self.compile_comprehension(
                &comprehension.generators,
                comprehension.key.as_deref(),
                &comprehension.value,
                BUILD_MAP,
                MAP_ADD,
                false,
            )?,
            Expr::Generator(generator) => self.compile_generator_expression(generator)?,
            Expr::Yield(expression) => {
                if self.unit.flags & (CO_GENERATOR | CO_ASYNC_GENERATOR) == 0 {
                    return Err(unsupported("yield outside generator code"));
                }
                if let Some(value) = &expression.value {
                    self.compile_expression(value)?;
                } else {
                    let none = self.add_constant(Constant::None)?;
                    self.emit(LOAD_CONST, none, 1)?;
                }
                self.assembler
                    .set_location(self.source_location(expression.range));
                if self.unit.flags & CO_ASYNC_GENERATOR != 0 {
                    self.emit_unary_intrinsic(UnaryIntrinsic::ASYNC_GEN_WRAP)?;
                }
                self.emit(YIELD_VALUE, 0, 0)?;
                let wrapper_is_only_exception_region =
                    self.control.generator_region_start.is_some()
                        && self.control.active_exception_region_exclusions.is_empty()
                        && self.control.active_with_region_exclusions.is_empty();
                self.emit_resume(
                    ResumeLocation::AFTER_YIELD,
                    wrapper_is_only_exception_region,
                )?;
            }
            Expr::YieldFrom(expression) => {
                if self.unit.flags & CO_GENERATOR == 0 {
                    return Err(unsupported("yield from outside generator code"));
                }
                self.compile_expression(&expression.value)?;
                self.emit(GET_YIELD_FROM_ITER, 0, 0)?;
                let none = self.add_constant(Constant::None)?;
                self.emit(LOAD_CONST, none, 1)?;
                let send = self.assembler.label();
                let end = self.assembler.label();
                let yielded = self.assembler.label();
                let yielded_end = self.assembler.label();
                let cleanup = self.assembler.label();
                self.assembler.mark(send);
                self.emit_jump_forward(SEND, end, 0)?;
                self.assembler.mark(yielded);
                self.emit(YIELD_VALUE, 1, 0)?;
                self.assembler.mark(yielded_end);
                self.emit_resume(
                    ResumeLocation::AFTER_YIELD_FROM,
                    self.control.generator_region_start.is_none(),
                )?;
                self.emit_jump_backward(JUMP_BACKWARD_NO_INTERRUPT, send, 0)?;
                self.assembler.mark(cleanup);
                self.assembler.add_exception_region(
                    yielded,
                    yielded_end,
                    cleanup,
                    (starting_depth + 2).cast_unsigned(),
                    false,
                );
                self.assembler
                    .set_location(self.source_location(expression.range));
                self.set_depth(starting_depth + 3);
                self.emit(CLEANUP_THROW, 0, -1)?;
                self.assembler.mark(end);
                self.set_depth(starting_depth + 2);
                self.emit(END_SEND, 0, -1)?;
            }
            Expr::Await(expression) => self.compile_await(expression)?,
            Expr::TString(tstring) => self.compile_tstring(tstring)?,
            Expr::Starred(starred) => {
                self.compile_expression(&starred.value)?;
                self.emit(UNPACK_SEQUENCE, 1, 0)?;
            }
            Expr::IpyEscapeCommand(_)
            | Expr::List(_)
            | Expr::Name(_)
            | Expr::Subscript(_)
            | Expr::Tuple(_)
            | Expr::Attribute(_) => return Err(unsupported(expression_name(expression))),
        }

        let deferred_restores = self
            .control
            .pending_comprehension_restores
            .iter()
            .map(|(indices, _)| indices.len())
            .sum::<usize>();
        let expected_depth = starting_depth
            + 1
            + if self.control.defer_async_comprehension_restore {
                i32::try_from(deferred_restores).unwrap()
            } else {
                0
            };
        if self.control.depth != expected_depth {
            return Err(CompileError::Internal(format!(
                "expression changed stack depth from {starting_depth} to {}",
                self.control.depth
            )));
        }
        Ok(())
    }

    fn compile_fstring(
        &mut self,
        fstring: &ruff_python_ast::ExprFString,
    ) -> Result<(), CompileError> {
        fn append_literal(
            pending: &mut Option<(String, ruff_text_size::TextRange)>,
            value: &str,
            range: ruff_text_size::TextRange,
        ) {
            if let Some((pending_value, pending_range)) = pending {
                pending_value.push_str(value);
                *pending_range = ruff_text_size::TextRange::new(pending_range.start(), range.end());
            } else {
                *pending = Some((value.to_string(), range));
            }
        }

        fn flush_literal(
            compiler: &mut Compiler,
            pending: &mut Option<(String, ruff_text_size::TextRange)>,
            value_count: &mut usize,
            large: bool,
            outer_location: SourceLocation,
        ) -> Result<(), CompileError> {
            let Some((value, range)) = pending.take() else {
                return Ok(());
            };
            if value.is_empty() {
                return Ok(());
            }
            compiler
                .assembler
                .set_location(compiler.source_location(range));
            compiler.load_string_constant(&value)?;
            *value_count += 1;
            if large {
                compiler.assembler.set_location(outer_location);
                compiler.emit(LIST_APPEND, 1, -1)?;
            }
            Ok(())
        }

        let mut component_count = 0_usize;
        let mut has_pending_literal = false;
        let mut has_interpolation = false;
        for part in &fstring.value {
            match part {
                FStringPart::Literal(literal) => {
                    has_pending_literal |= !literal.value.is_empty();
                }
                FStringPart::FString(fstring) => {
                    for element in &fstring.elements {
                        match element {
                            InterpolatedStringElement::Literal(literal) => {
                                has_pending_literal |= !literal.value.is_empty();
                            }
                            InterpolatedStringElement::Interpolation(interpolation) => {
                                has_interpolation = true;
                                if interpolation.debug_text.is_some() {
                                    has_pending_literal = true;
                                }
                                if std::mem::take(&mut has_pending_literal) {
                                    component_count += 1;
                                }
                                component_count += 1;
                            }
                        }
                    }
                }
            }
        }
        component_count += usize::from(has_pending_literal);
        let large = component_count > 30;
        let outer_location = self.source_location(fstring.range);
        let single_literal_range = if component_count == 1 && !has_interpolation {
            Some(self.fstring_result_range(fstring))
        } else {
            None
        };
        if large {
            self.assembler.set_location(outer_location);
            self.load_string_constant("")?;
            let join = self.name_index("join")?;
            self.emit(LOAD_ATTR, (join << 1) | 1, 1)?;
            self.emit(BUILD_LIST, 0, 1)?;
        }

        let mut value_count = 0_usize;
        let mut pending_literal = None;
        for part in &fstring.value {
            match part {
                FStringPart::Literal(literal) => {
                    append_literal(
                        &mut pending_literal,
                        &literal.value,
                        single_literal_range.unwrap_or(literal.range),
                    );
                }
                FStringPart::FString(fstring) => {
                    for element in &fstring.elements {
                        match element {
                            InterpolatedStringElement::Literal(literal) => {
                                append_literal(
                                    &mut pending_literal,
                                    &literal.value,
                                    single_literal_range.unwrap_or(literal.range),
                                );
                            }
                            InterpolatedStringElement::Interpolation(interpolation) => {
                                if let Some(debug) = &interpolation.debug_text {
                                    let debug_text = strip_expression_comments(&format!(
                                        "{}{}{}",
                                        debug.leading(),
                                        debug.expression(),
                                        debug.trailing()
                                    ));
                                    append_literal(
                                        &mut pending_literal,
                                        &debug_text,
                                        debug_text_range(interpolation),
                                    );
                                }
                                flush_literal(
                                    self,
                                    &mut pending_literal,
                                    &mut value_count,
                                    large,
                                    outer_location,
                                )?;
                                self.compile_interpolation(interpolation)?;
                                value_count += 1;
                                if large {
                                    self.assembler.set_location(outer_location);
                                    self.emit(LIST_APPEND, 1, -1)?;
                                }
                            }
                        }
                    }
                }
            }
        }
        flush_literal(
            self,
            &mut pending_literal,
            &mut value_count,
            large,
            outer_location,
        )?;
        self.assembler.set_location(outer_location);
        if large {
            self.emit(CALL, 1, -2)
        } else {
            self.finish_interpolated_string(value_count, "f-string component count")
        }
    }

    fn compile_tstring(
        &mut self,
        tstring: &ruff_python_ast::ExprTString,
    ) -> Result<(), CompileError> {
        let mut strings = vec![String::new()];
        let mut interpolations = Vec::new();
        for element in tstring.value.elements() {
            match element {
                InterpolatedStringElement::Literal(literal) => {
                    strings.last_mut().unwrap().push_str(&literal.value);
                }
                InterpolatedStringElement::Interpolation(interpolation) => {
                    if let Some(debug) = &interpolation.debug_text {
                        strings
                            .last_mut()
                            .unwrap()
                            .push_str(&strip_expression_comments(&format!(
                                "{}{}{}",
                                debug.leading(),
                                debug.expression(),
                                debug.trailing()
                            )));
                    }
                    interpolations.push(interpolation);
                    strings.push(String::new());
                }
            }
        }

        let strings = strings
            .into_iter()
            .map(Constant::String)
            .collect::<Vec<_>>();
        for string in &strings {
            self.add_constant(string.clone())?;
        }
        self.emit_deferred_constant(Constant::Tuple(strings))?;
        for interpolation in &interpolations {
            self.compile_expression(&interpolation.expression)?;
            self.assembler
                .set_location(self.source_location(interpolation.range));
            let expression_text = self.interpolation_expression_text(interpolation);
            self.load_string_constant(&expression_text)?;

            let conversion = if interpolation.debug_text.is_some()
                && interpolation.format_spec.is_none()
                && interpolation.conversion == ConversionFlag::None
            {
                ConversionFlag::Repr
            } else {
                interpolation.conversion
            };
            let mut argument = 2 | match conversion {
                ConversionFlag::None => 0,
                ConversionFlag::Str => 4,
                ConversionFlag::Repr => 8,
                ConversionFlag::Ascii => 12,
            };
            let effect = if let Some(format_spec) = &interpolation.format_spec {
                self.compile_interpolated_elements(&format_spec.elements, format_spec.range)?;
                argument |= 1;
                -2
            } else {
                -1
            };
            self.assembler
                .set_location(self.source_location(interpolation.range));
            self.emit(BUILD_INTERPOLATION, argument, effect)?;
        }
        self.assembler
            .set_location(self.source_location(tstring.range));
        if interpolations.is_empty() {
            self.emit_deferred_constant(Constant::Tuple(Vec::new()))?;
        } else {
            self.emit_build(BUILD_TUPLE, interpolations.len())?;
        }
        self.emit(BUILD_TEMPLATE, 0, -1)
    }

    fn interpolation_expression_text(
        &self,
        interpolation: &ruff_python_ast::InterpolatedElement,
    ) -> String {
        let start = usize::from(interpolation.range.start()) + 1;
        let expression_end = usize::from(interpolation.expression.range().end());
        // CPython truncates the stored expression text at a top-level `!=`
        // when the interpolation has a format specifier.
        if interpolation.format_spec.is_some()
            && interpolation.debug_text.is_none()
            && interpolation.conversion == ConversionFlag::None
            && let Expr::Compare(comparison) = interpolation.expression.as_ref()
            && comparison.ops.contains(&CmpOp::NotEq)
            && let Some(offset) = self.context.source[start..expression_end].find("!=")
        {
            return strip_expression_comments(
                self.context.source[start..start + offset].trim_end(),
            );
        }
        let interpolation_end = usize::from(interpolation.range.end()).saturating_sub(1);
        let trailing = &self.context.source[expression_end..interpolation_end];
        let delimiter = if interpolation.debug_text.is_some() {
            trailing.find('=')
        } else if interpolation.conversion != ConversionFlag::None {
            trailing.find('!')
        } else if interpolation.format_spec.is_some() {
            trailing.find(':')
        } else {
            None
        };
        let end = delimiter.map_or(interpolation_end, |offset| expression_end + offset);
        strip_expression_comments(&self.context.source[start..end])
            .trim_end()
            .to_string()
    }

    fn compile_interpolated_elements(
        &mut self,
        elements: &ruff_python_ast::InterpolatedStringElements,
        range: ruff_text_size::TextRange,
    ) -> Result<(), CompileError> {
        fn append_literal(
            pending: &mut Option<(String, ruff_text_size::TextRange)>,
            value: &str,
            range: ruff_text_size::TextRange,
        ) {
            if let Some((pending_value, pending_range)) = pending {
                pending_value.push_str(value);
                *pending_range = ruff_text_size::TextRange::new(pending_range.start(), range.end());
            } else {
                *pending = Some((value.to_string(), range));
            }
        }

        fn flush_literal(
            compiler: &mut Compiler,
            pending: &mut Option<(String, ruff_text_size::TextRange)>,
            value_count: &mut usize,
        ) -> Result<(), CompileError> {
            let Some((value, range)) = pending.take() else {
                return Ok(());
            };
            compiler
                .assembler
                .set_location(compiler.source_location(range));
            compiler.load_string_constant(&value)?;
            *value_count += 1;
            Ok(())
        }

        let mut value_count = 0_usize;
        let mut pending_literal = None;
        for element in elements {
            match element {
                InterpolatedStringElement::Literal(literal) => {
                    append_literal(&mut pending_literal, &literal.value, literal.range);
                }
                InterpolatedStringElement::Interpolation(interpolation) => {
                    if let Some(debug) = &interpolation.debug_text {
                        let debug_text = strip_expression_comments(&format!(
                            "{}{}{}",
                            debug.leading(),
                            debug.expression(),
                            debug.trailing()
                        ));
                        append_literal(
                            &mut pending_literal,
                            &debug_text,
                            debug_text_range(interpolation),
                        );
                    }
                    flush_literal(self, &mut pending_literal, &mut value_count)?;
                    self.compile_interpolation(interpolation)?;
                    value_count += 1;
                }
            }
        }
        flush_literal(self, &mut pending_literal, &mut value_count)?;
        let range = ruff_text_size::TextRange::new(
            ruff_text_size::TextSize::new(u32::from(range.start()).saturating_sub(1)),
            range.end(),
        );
        self.assembler.set_location(self.source_location(range));
        self.finish_interpolated_string(value_count, "format specification component count")
    }

    fn compile_interpolation(
        &mut self,
        interpolation: &ruff_python_ast::InterpolatedElement,
    ) -> Result<(), CompileError> {
        self.compile_expression(&interpolation.expression)?;
        self.assembler
            .set_location(self.source_location(interpolation.range));
        match interpolation.conversion {
            ConversionFlag::None
                if interpolation.debug_text.is_some() && interpolation.format_spec.is_none() =>
            {
                self.emit_conversion(Conversion::REPR)?;
            }
            ConversionFlag::None => {}
            ConversionFlag::Str => self.emit_conversion(Conversion::STR)?,
            ConversionFlag::Repr => self.emit_conversion(Conversion::REPR)?,
            ConversionFlag::Ascii => self.emit_conversion(Conversion::ASCII)?,
        }
        if let Some(format_spec) = &interpolation.format_spec {
            self.compile_interpolated_elements(&format_spec.elements, format_spec.range)?;
            self.assembler
                .set_location(self.source_location(interpolation.range));
            self.emit(FORMAT_WITH_SPEC, 0, -1)
        } else {
            self.emit(FORMAT_SIMPLE, 0, 0)
        }
    }

    fn compile_optimized_percent_format(
        &mut self,
        expression: &ExprBinOp,
    ) -> Result<bool, CompileError> {
        let Some(parts) = optimized_percent_format(expression) else {
            return Ok(false);
        };

        let expression_location = self.source_location(expression.range);
        let part_count = parts.len();
        for part in parts {
            match part {
                PercentFormatPart::Literal(value) => {
                    // The preprocessing pass synthesizes these constants without
                    // source locations.
                    self.assembler.set_location(SourceLocation::NONE);
                    self.load_string_constant(&value)?;
                }
                PercentFormatPart::Formatted {
                    expression,
                    conversion,
                    format_spec,
                } => {
                    self.compile_expression(expression)?;
                    let location = self.source_location(expression.range());
                    self.assembler.set_location(location);
                    self.emit_conversion(conversion)?;
                    if let Some(format_spec) = format_spec {
                        self.assembler.set_location(SourceLocation::NONE);
                        self.load_string_constant(&format_spec)?;
                        self.assembler.set_location(location);
                        self.emit(FORMAT_WITH_SPEC, 0, -1)?;
                    } else {
                        self.emit(FORMAT_SIMPLE, 0, 0)?;
                    }
                }
            }
        }

        self.assembler.set_location(expression_location);
        match part_count {
            0 => self.load_string_constant("")?,
            1 => {}
            count => self.emit(
                BUILD_STRING,
                to_u32(count, "percent-format component count")?,
                1 - i32::try_from(count).unwrap(),
            )?,
        }
        Ok(true)
    }

    pub(super) fn load_string_constant(&mut self, value: &str) -> Result<(), CompileError> {
        let constant = self.add_constant(Constant::String(value.to_string()))?;
        self.emit(LOAD_CONST, constant, 1)
    }

    fn finish_interpolated_string(
        &mut self,
        value_count: usize,
        description: &str,
    ) -> Result<(), CompileError> {
        match value_count {
            0 => self.load_string_constant(""),
            1 => Ok(()),
            count => self.emit(
                BUILD_STRING,
                to_u32(count, description)?,
                1 - i32::try_from(count).unwrap(),
            ),
        }
    }

    fn compile_await(
        &mut self,
        expression: &ruff_python_ast::ExprAwait,
    ) -> Result<(), CompileError> {
        if self.unit.flags & (CO_COROUTINE | CO_ASYNC_GENERATOR) == 0 {
            return Err(unsupported("await outside coroutine code"));
        }

        self.compile_expression(&expression.value)?;
        self.compile_awaitable_on_stack(0)
    }

    pub(super) fn compile_awaitable_on_stack(
        &mut self,
        get_awaitable_argument: u32,
    ) -> Result<(), CompileError> {
        let base_depth = self.control.depth - 1;
        let cleanup_location = self.assembler.location();
        self.emit(GET_AWAITABLE, get_awaitable_argument, 0)?;
        let none = self.add_constant(Constant::None)?;
        self.emit(LOAD_CONST, none, 1)?;

        let send = self.assembler.label();
        let end = self.assembler.label();
        let yielded = self.assembler.label();
        let yielded_end = self.assembler.label();
        let cleanup = self.assembler.label();
        self.assembler.mark(send);
        self.emit_jump_forward(SEND, end, 0)?;
        self.assembler.mark(yielded);
        self.emit(YIELD_VALUE, 1, 0)?;
        self.assembler.mark(yielded_end);
        self.emit_resume(ResumeLocation::AFTER_AWAIT, false)?;
        self.emit_jump_backward(JUMP_BACKWARD_NO_INTERRUPT, send, 0)?;

        self.assembler.mark(cleanup);
        self.assembler.add_exception_region(
            yielded,
            yielded_end,
            cleanup,
            (base_depth + 2).cast_unsigned(),
            false,
        );
        self.assembler.set_location(cleanup_location);
        self.set_depth(base_depth + 3);
        self.emit(CLEANUP_THROW, 0, -1)?;
        self.assembler.mark(end);
        self.set_depth(base_depth + 2);
        self.emit(END_SEND, 0, -1)?;
        Ok(())
    }

    pub(super) fn emit_build(&mut self, opcode: Opcode, length: usize) -> Result<(), CompileError> {
        let length = to_u32(length, "collection element count")?;
        self.emit(opcode, length, 1 - i32::try_from(length).unwrap())
    }

    fn compile_call(&mut self, call: &ruff_python_ast::ExprCall) -> Result<(), CompileError> {
        if let Some(callable) = optimized_generator_callable(call) {
            return self.compile_optimized_generator_call(call, callable);
        }

        let has_starred = call.arguments.args.iter().any(Expr::is_starred_expr);
        let has_keyword_unpack = call
            .arguments
            .keywords
            .iter()
            .any(|keyword| keyword.arg.is_none());
        let direct_method = if let Some(attribute) = self.direct_method_attribute(call) {
            if !self.compile_super_attribute(attribute, true)? {
                self.compile_expression(&attribute.value)?;
                let index = self.name_index(attribute.attr.as_str())?;
                self.assembler
                    .set_location(self.source_location(self.attribute_opcode_range(attribute)));
                self.emit(LOAD_ATTR, (index << 1) | 1, 1)?;
            }
            true
        } else {
            false
        };
        if !direct_method && !self.compile_direct_global_callable(&call.func)? {
            let newly_owned_callable = if call
                .arguments
                .args
                .iter()
                .chain(call.arguments.keywords.iter().map(|keyword| &keyword.value))
                .any(expression_contains_inlined_comprehension)
                && let Expr::Name(name) = call.func.as_ref()
            {
                let name = self.required_name_id(name.id.as_str());
                self.symbols.owned_load_locals.insert(name)
            } else {
                false
            };
            self.compile_expression(&call.func)?;
            if newly_owned_callable && let Expr::Name(name) = call.func.as_ref() {
                let name = self.required_name_id(name.id.as_str());
                self.symbols.owned_load_locals.remove(name);
            }
            self.assembler
                .set_location(self.source_location(call.func.range()));
            self.emit(PUSH_NULL, 0, 1)?;
        }
        self.assembler
            .set_location(self.source_location(self.call_opcode_range(call)));

        if !has_starred && !has_keyword_unpack {
            if call.arguments.keywords.len() > 15 {
                let keyword_names = call
                    .arguments
                    .keywords
                    .iter()
                    .map(|keyword| {
                        self.add_constant(Constant::String(
                            keyword.arg.as_ref().unwrap().as_str().to_string(),
                        ))
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                if call.arguments.args.is_empty() {
                    self.add_constant(Constant::None)?;
                    self.emit_deferred_constant(Constant::Tuple(Vec::new()))?;
                } else {
                    self.compile_collection(&call.arguments.args, CollectionKind::Tuple)?;
                }
                self.emit(BUILD_MAP, 0, 1)?;
                for (keyword, name) in call.arguments.keywords.iter().zip(keyword_names) {
                    self.assembler
                        .set_location(self.source_location(self.call_opcode_range(call)));
                    self.emit(LOAD_CONST, name, 1)?;
                    self.compile_expression(&keyword.value)?;
                    self.assembler
                        .set_location(self.source_location(keyword.value.range()));
                    self.emit(MAP_ADD, 1, -2)?;
                }
                self.assembler
                    .set_location(self.source_location(self.call_opcode_range(call)));
                return self.emit(CALL_FUNCTION_EX, 0, -3);
            }
            for argument in &call.arguments.args {
                self.compile_expression(argument)?;
            }
            for keyword in &call.arguments.keywords {
                self.compile_expression(&keyword.value)?;
            }
            let argument_count = call.arguments.args.len() + call.arguments.keywords.len();
            let argument_count = to_u32(argument_count, "call argument count")?;
            if call.arguments.keywords.is_empty() {
                return self.emit(
                    CALL,
                    argument_count,
                    -i32::try_from(argument_count).unwrap() - 1,
                );
            }

            let keyword_names = call
                .arguments
                .keywords
                .iter()
                .map(|keyword| Constant::String(keyword.arg.as_ref().unwrap().as_str().to_string()))
                .collect();
            let names = self.add_interned_string_tuple(keyword_names)?;
            if direct_method {
                let range = if let Expr::Attribute(attribute) = call.func.as_ref() {
                    self.attribute_opcode_range(attribute)
                } else {
                    call.func.range()
                };
                self.assembler.set_location(self.source_location(range));
            }
            self.emit(LOAD_CONST, names, 1)?;
            self.assembler
                .set_location(self.source_location(self.call_opcode_range(call)));
            return self.emit(
                CALL_KW,
                argument_count,
                -i32::try_from(argument_count).unwrap() - 2,
            );
        }

        if call.arguments.args.is_empty() {
            self.emit_deferred_constant(Constant::Tuple(Vec::new()))?;
        } else if call.arguments.args.len() == 1 {
            if let Expr::Starred(starred) = &call.arguments.args[0] {
                self.compile_expression(&starred.value)?;
            } else {
                self.compile_collection(&call.arguments.args, CollectionKind::Tuple)?;
            }
        } else {
            self.compile_collection(&call.arguments.args, CollectionKind::Tuple)?;
        }

        if call.arguments.keywords.is_empty() {
            self.emit(PUSH_NULL, 0, 1)?;
        } else {
            let mut have_dict = false;
            let mut named_start = 0;
            for (index, keyword) in call.arguments.keywords.iter().enumerate() {
                if keyword.arg.is_some() {
                    continue;
                }
                if named_start < index {
                    self.compile_keyword_map(call, &call.arguments.keywords[named_start..index])?;
                    if have_dict {
                        self.emit(DICT_MERGE, 1, -1)?;
                    }
                    have_dict = true;
                }
                if !have_dict {
                    self.emit(BUILD_MAP, 0, 1)?;
                    have_dict = true;
                }
                self.compile_expression(&keyword.value)?;
                self.assembler
                    .set_location(self.source_location(self.call_opcode_range(call)));
                self.emit(DICT_MERGE, 1, -1)?;
                named_start = index + 1;
            }
            if named_start < call.arguments.keywords.len() {
                self.compile_keyword_map(call, &call.arguments.keywords[named_start..])?;
                if have_dict {
                    self.emit(DICT_MERGE, 1, -1)?;
                }
            }
        }
        self.emit(CALL_FUNCTION_EX, 0, -3)
    }

    fn compile_optimized_generator_call(
        &mut self,
        call: &ruff_python_ast::ExprCall,
        callable: &str,
    ) -> Result<(), CompileError> {
        let (common_constant, initial_result, collect_tuple) = match callable {
            "all" => (CommonConstant::BUILTIN_ALL, true, false),
            "any" => (CommonConstant::BUILTIN_ANY, false, false),
            "tuple" => (CommonConstant::BUILTIN_TUPLE, false, true),
            _ => unreachable!("optimized generator call must use all, any, or tuple"),
        };
        let callable_location = self.source_location(call.func.range());
        let base_depth = self.control.depth;
        let fallback = self.assembler.label();
        let end = self.assembler.label();

        self.compile_expression(&call.func)?;
        self.assembler.set_location(callable_location);
        self.emit(COPY, 1, 1)?;
        self.emit_common_constant(common_constant)?;
        self.emit(IS_OP, 0, -1)?;
        self.emit_jump_forward(POP_JUMP_IF_FALSE, fallback, -1)?;
        self.emit(NOT_TAKEN, 0, 0)?;
        self.emit(POP_TOP, 0, -1)?;

        if collect_tuple {
            self.emit(BUILD_LIST, 0, 1)?;
        }
        self.compile_expression(&call.arguments.args[0])?;

        let loop_start = self.assembler.label();
        let exhausted = self.assembler.label();
        let iterator_depth = self.control.depth;
        self.assembler.mark(loop_start);
        self.assembler.set_location(callable_location);
        self.emit_jump_forward(FOR_ITER, exhausted, 1)?;
        if collect_tuple {
            self.emit(LIST_APPEND, 2, -1)?;
            self.emit_jump_backward(JUMP_BACKWARD, loop_start, 0)?;
        } else {
            self.emit(TO_BOOL, 0, 0)?;
            let matched = self.assembler.label();
            self.emit_jump_forward(
                if initial_result {
                    POP_JUMP_IF_FALSE
                } else {
                    POP_JUMP_IF_TRUE
                },
                matched,
                -1,
            )?;
            self.emit(NOT_TAKEN, 0, 0)?;
            self.emit_jump_backward(JUMP_BACKWARD, loop_start, 0)?;
            self.assembler.mark(matched);
            self.emit(POP_ITER, 0, -1)?;
            let result = self.add_constant(Constant::Bool(!initial_result))?;
            self.emit(LOAD_CONST, result, 1)?;
            self.emit_jump_forward(JUMP_FORWARD, end, 0)?;
        }

        self.assembler.mark(exhausted);
        self.set_depth(iterator_depth + 1);
        self.assembler.set_location(callable_location);
        self.emit(END_FOR, 0, -1)?;
        self.emit(POP_ITER, 0, -1)?;
        self.assembler.set_location(callable_location);
        if collect_tuple {
            self.emit_unary_intrinsic(UnaryIntrinsic::LIST_TO_TUPLE)?;
        } else {
            let result = self.add_constant(Constant::Bool(initial_result))?;
            self.emit(LOAD_CONST, result, 1)?;
        }
        self.emit_jump_forward(JUMP_FORWARD, end, 0)?;

        self.assembler.mark(fallback);
        self.set_depth(base_depth + 1);
        self.assembler.set_location(callable_location);
        self.emit(PUSH_NULL, 0, 1)?;
        self.compile_expression(&call.arguments.args[0])?;
        self.assembler
            .set_location(self.source_location(self.call_opcode_range(call)));
        self.emit(CALL, 1, -2)?;

        self.assembler.mark(end);
        self.set_depth(base_depth + 1);
        Ok(())
    }

    fn compile_keyword_map(
        &mut self,
        call: &ruff_python_ast::ExprCall,
        keywords: &[Keyword],
    ) -> Result<(), CompileError> {
        debug_assert!(!keywords.is_empty() && keywords.iter().all(|keyword| keyword.arg.is_some()));
        let call_location = self.source_location(self.call_opcode_range(call));
        if keywords.len() * 2 > 30 {
            self.assembler.set_location(SourceLocation::NONE);
            self.emit(BUILD_MAP, 0, 1)?;
            for keyword in keywords {
                let name = keyword.arg.as_ref().unwrap();
                let key = self.add_constant(Constant::String(name.as_str().to_string()))?;
                self.assembler.set_location(call_location);
                self.emit(LOAD_CONST, key, 1)?;
                self.compile_expression(&keyword.value)?;
                self.assembler.set_location(SourceLocation::NONE);
                self.emit(MAP_ADD, 1, -2)?;
            }
        } else {
            for keyword in keywords {
                let name = keyword.arg.as_ref().unwrap();
                let key = self.add_constant(Constant::String(name.as_str().to_string()))?;
                self.assembler.set_location(call_location);
                self.emit(LOAD_CONST, key, 1)?;
                self.compile_expression(&keyword.value)?;
            }
            self.assembler.set_location(call_location);
            let count = to_u32(keywords.len(), "keyword count")?;
            self.emit(
                BUILD_MAP,
                count,
                1 - i32::try_from(keywords.len() * 2).unwrap(),
            )?;
        }
        Ok(())
    }

    pub(super) fn imported_module_attribute(
        &self,
        attribute: &ruff_python_ast::ExprAttribute,
    ) -> bool {
        let Expr::Name(name) = attribute.value.as_ref() else {
            return false;
        };
        // CPython only checks whether the name is import-originated in the module symbol table.
        self.is_imported(name.id.as_str())
    }

    fn compile_super_attribute(
        &mut self,
        attribute: &ruff_python_ast::ExprAttribute,
        method: bool,
    ) -> Result<bool, CompileError> {
        let Expr::Call(call) = attribute.value.as_ref() else {
            return Ok(false);
        };
        let Expr::Name(super_name) = call.func.as_ref() else {
            return Ok(false);
        };
        if super_name.id.as_str() != "super"
            || attribute.attr.as_str() == "__class__"
            || !call.arguments.keywords.is_empty()
            || self.is_imported("super")
            || self.is_imported(SHADOWED_SUPER_SENTINEL)
        {
            return Ok(false);
        }
        let explicit = match call.arguments.args.as_ref() {
            [] => false,
            [first, second]
                if !matches!(first, Expr::Starred(_)) && !matches!(second, Expr::Starred(_)) =>
            {
                true
            }
            _ => return Ok(false),
        };
        let globally_resolved = match &self.unit.scope {
            Scope::Module => true,
            Scope::Function {
                indices,
                free_indices,
                globals,
                ..
            } => {
                self.contains_name(globals, "super")
                    && self.scope_index(indices, "super").is_none()
                    && self.scope_index(free_indices, "super").is_none()
            }
            Scope::Class { bound_names, .. } => !self.contains_name(bound_names, "super"),
        };
        if !globally_resolved {
            return Ok(false);
        }

        if explicit {
            self.assembler
                .set_location(self.source_location(super_name.range));
            self.load_name("super")?;
            self.compile_expression(&call.arguments.args[0])?;
            self.compile_expression(&call.arguments.args[1])?;
            self.assembler
                .set_location(self.source_location(attribute.range));
            let attribute_index = self.name_index(attribute.attr.as_str())?;
            self.emit(
                LOAD_SUPER_ATTR,
                (attribute_index << 2) | 2 | u32::from(method),
                if method { -1 } else { -2 },
            )?;
            self.assembler
                .set_location(self.source_location(self.attribute_opcode_range(attribute)));
            self.emit(NOP, 0, 0)?;
            return Ok(true);
        }

        if self.unit.arg_count == 0 {
            return Ok(false);
        }
        let (class_index, first_parameter) = match &self.unit.scope {
            Scope::Function {
                indices,
                free_indices,
                globals,
                ..
            } if self.contains_name(globals, "super")
                && self.scope_index(indices, "super").is_none() =>
            {
                let Some(class_index) = self.scope_index(free_indices, "__class__") else {
                    return Ok(false);
                };
                let Some(first_parameter) = self.symbols.locals.first() else {
                    return Ok(false);
                };
                let Some(first_parameter) = self.scope_index(indices, first_parameter) else {
                    return Ok(false);
                };
                (class_index, first_parameter)
            }
            _ => return Ok(false),
        };

        self.assembler
            .set_location(self.source_location(super_name.range));
        let super_index = self.name_index("super")?;
        self.emit(LOAD_GLOBAL, super_index << 1, 1)?;
        self.assembler
            .set_location(self.source_location(call.range));
        self.emit(LOAD_DEREF, class_index, 1)?;
        self.emit(LOAD_FAST, first_parameter, 1)?;
        self.assembler
            .set_location(self.source_location(attribute.range));
        let attribute_index = self.name_index(attribute.attr.as_str())?;
        self.emit(
            LOAD_SUPER_ATTR,
            (attribute_index << 2) | u32::from(method),
            if method { -1 } else { -2 },
        )?;
        self.assembler
            .set_location(self.source_location(self.attribute_opcode_range(attribute)));
        self.emit(NOP, 0, 0)?;
        Ok(true)
    }

    fn compile_direct_global_callable(&mut self, expression: &Expr) -> Result<bool, CompileError> {
        if self.unit.annotation_classdict_index.is_some() {
            return Ok(false);
        }
        let Expr::Name(name) = expression else {
            return Ok(false);
        };
        let is_global = match &self.unit.scope {
            Scope::Function {
                indices,
                free_indices,
                globals,
                ..
            } => {
                self.contains_name(globals, name.id.as_str())
                    || (self.scope_index(indices, name.id.as_str()).is_none()
                        && self.scope_index(free_indices, name.id.as_str()).is_none())
            }
            Scope::Module | Scope::Class { .. } => false,
        };
        if !is_global {
            return Ok(false);
        }
        self.assembler
            .set_location(self.source_location(expression.range()));
        let index = self.name_index(name.id.as_str())?;
        self.emit(LOAD_GLOBAL, (index << 1) | 1, 2)?;
        Ok(true)
    }

    fn compile_lambda(&mut self, lambda: &ruff_python_ast::ExprLambda) -> Result<(), CompileError> {
        let empty_parameters = ruff_python_ast::Parameters::default();
        let parameters = lambda.parameters.as_deref().unwrap_or(&empty_parameters);
        if parameters
            .iter()
            .any(|parameter| parameter.annotation().is_some())
        {
            return Err(unsupported("lambda parameter annotation"));
        }

        let positional_defaults: Vec<_> = parameters
            .posonlyargs
            .iter()
            .chain(&parameters.args)
            .filter_map(|parameter| parameter.default.as_deref())
            .collect();
        if !positional_defaults.is_empty() {
            if let Some(defaults) = positional_defaults
                .iter()
                .map(|default| fold_constant(default))
                .collect::<Option<Vec<_>>>()
            {
                for default in &positional_defaults {
                    self.record_folded_value(default)?;
                }
                self.assembler
                    .set_location(self.source_location(lambda.range));
                self.emit_deferred_constant(Constant::Tuple(defaults))?;
            } else {
                for default in &positional_defaults {
                    self.compile_expression(default)?;
                }
                self.emit_build(BUILD_TUPLE, positional_defaults.len())?;
            }
        }
        let keyword_defaults: Vec<_> = parameters
            .kwonlyargs
            .iter()
            .filter_map(|parameter| {
                parameter
                    .default
                    .as_deref()
                    .map(|default| (parameter.name().as_str(), default))
            })
            .collect();
        if !keyword_defaults.is_empty() {
            for (name, default) in &keyword_defaults {
                let name = self.add_constant(Constant::String(self.mangled_name(name)))?;
                self.emit(LOAD_CONST, name, 1)?;
                self.compile_expression(default)?;
            }
            let count = to_u32(keyword_defaults.len(), "keyword default count")?;
            self.emit(BUILD_MAP, count, 1 - i32::try_from(count).unwrap() * 2)?;
        }

        let analysis = analyze_lambda_scope(lambda);
        let freevars = analysis
            .required
            .iter()
            .filter(|name| self.can_provide_closure(name))
            .cloned()
            .collect::<BTreeSet<_>>();
        let globals = analysis.required.difference(&freevars).cloned().collect();
        let closure_names = freevars.iter().cloned().collect::<Vec<_>>();
        let plan = LoweringFunctionPlan {
            locals: analysis.locals,
            globals,
            cellvars: analysis.cellvars,
            inlined_comprehension_cellvars: analysis.inlined_comprehension_cellvars,
            freevars,
            ..LoweringFunctionPlan::default()
        };

        let arg_count = to_u32(
            parameters.posonlyargs.len() + parameters.args.len(),
            "lambda argument count",
        )?;
        let positional_only = to_u32(
            parameters.posonlyargs.len(),
            "lambda positional-only argument count",
        )?;
        let keyword_only = to_u32(
            parameters.kwonlyargs.len(),
            "lambda keyword-only argument count",
        )?;
        let mut parameter_flags = (if parameters.vararg.is_some() {
            CO_VARARGS
        } else {
            0
        } | if parameters.kwarg.is_some() {
            CO_VARKEYWORDS
        } else {
            0
        } | if matches!(self.unit.scope, Scope::Function { .. }) {
            CO_NESTED
        } else if matches!(self.unit.scope, Scope::Class { .. }) {
            CO_METHOD
                | if self.unit.class_scope_is_nested {
                    CO_NESTED
                } else {
                    0
                }
        } else if !closure_names.is_empty()
            || !self.control.active_comprehension_cleanups.is_empty()
        {
            CO_NESTED
        } else {
            0
        }) | (self.unit.flags & SUPPORTED_FUTURE_FLAGS);
        if expression_contains_yield(&lambda.body) {
            parameter_flags |= CO_GENERATOR;
        }
        let qualified_name = self.child_qualified_name("<lambda>");
        let first_line_number = self.line_number(u32::from(lambda.range.start()));
        let mut child = Self::function(
            Arc::clone(&self.context),
            "<lambda>",
            qualified_name,
            first_line_number,
            &plan,
            self.unit.scope_id,
            arg_count,
            positional_only,
            keyword_only,
            parameter_flags,
        )?;
        child.unit.private_name.clone_from(&self.unit.private_name);
        child
            .symbols
            .imported_scope_names
            .clone_from(&self.symbols.imported_scope_names);
        child.emit_function_prologue()?;
        if parameter_flags & CO_GENERATOR != 0 {
            // CPython does not wrap generator lambdas in the stop-iteration
            // handler used by `def` generators and generator expressions.
            child.control.generator_region_start = None;
        }
        child.compile_expression(&lambda.body)?;
        child
            .assembler
            .set_location(child.source_location(lambda.body.range()));
        child.emit(RETURN_VALUE, 0, -1)?;
        let child = child.finish_inner(false)?;

        if !closure_names.is_empty() {
            self.emit_closure_tuple(&closure_names)?;
        }
        let constant = self.add_constant(Constant::Code(Box::new(child)))?;
        self.emit(LOAD_CONST, constant, 1)?;
        self.emit(MAKE_FUNCTION, 0, 0)?;
        if !closure_names.is_empty() {
            self.emit_function_attribute(FunctionAttribute::CLOSURE)?;
        }
        if !keyword_defaults.is_empty() {
            self.emit_function_attribute(FunctionAttribute::KWDEFAULTS)?;
        }
        if !positional_defaults.is_empty() {
            self.emit_function_attribute(FunctionAttribute::DEFAULTS)?;
        }
        Ok(())
    }

    pub(super) fn compile_comprehension(
        &mut self,
        generators: &[ruff_python_ast::Comprehension],
        key: Option<&Expr>,
        value: &Expr,
        build_opcode: Opcode,
        add_opcode: Opcode,
        discard_result: bool,
    ) -> Result<(), CompileError> {
        if generators.is_empty() {
            return Err(CompileError::Internal(
                "comprehension has no generators".to_string(),
            ));
        }
        let comprehension_cell_names = comprehension_cell_names(generators, key, value);
        let base_depth = self.control.depth;
        let comprehension_location = self.assembler.location();
        if generators[0].is_async {
            self.compile_expression(&generators[0].iter)?;
        } else {
            self.compile_iterable_expression(&generators[0].iter)?;
        }
        self.assembler
            .set_location(self.source_location(generators[0].iter.range()));
        self.emit(
            if generators[0].is_async {
                GET_AITER
            } else {
                GET_ITER
            },
            0,
            0,
        )?;
        self.assembler.set_location(comprehension_location);

        let mut temporary_names = Vec::new();
        for (index, generator) in generators.iter().enumerate() {
            collect_target_names(&generator.target, &mut temporary_names);
            // The first iterable runs before entering the inlined comprehension scope.
            if index > 0 {
                collect_nested_comprehension_target_names(&generator.iter, &mut temporary_names);
            }
            for condition in &generator.ifs {
                collect_nested_comprehension_target_names(condition, &mut temporary_names);
            }
        }
        if let Some(key) = key {
            collect_nested_comprehension_target_names(key, &mut temporary_names);
        }
        collect_nested_comprehension_target_names(value, &mut temporary_names);
        let active_temporary_names = temporary_names.clone();
        if matches!(self.unit.scope, Scope::Module) {
            // CPython assigns a hidden fast-local slot to module-scope walrus targets while
            // inlining a comprehension. Function-scope targets remain bindings in the
            // containing function and are not part of the comprehension's save set.
            if let Some(key) = key {
                collect_named_expression_target_names(key, &mut temporary_names);
            }
            collect_named_expression_target_names(value, &mut temporary_names);
            for generator in generators {
                for condition in &generator.ifs {
                    collect_named_expression_target_names(condition, &mut temporary_names);
                }
            }
        }
        let mut seen_temporaries = HashSet::new();
        temporary_names.retain(|name| seen_temporaries.insert(name.clone()));
        let mut temporary_indices = Vec::with_capacity(temporary_names.len());
        for name in &temporary_names {
            let index = self.ensure_temporary(name)?;
            temporary_indices.push(index);
            self.emit(LOAD_FAST_AND_CLEAR, index, 1)?;
            if comprehension_cell_names.contains(name) && self.is_cell(name) {
                self.emit(MAKE_CELL, index, 0)?;
            }
        }
        if !temporary_names.is_empty() {
            self.emit(
                SWAP,
                to_u32(temporary_names.len() + 1, "comprehension local count")?,
                0,
            )?;
        }
        let active_temporary_ids: Vec<_> = active_temporary_names
            .iter()
            .map(|name| self.required_name_id(name))
            .collect();
        self.symbols
            .active_temporaries
            .extend(active_temporary_ids.iter().copied());

        let protected_start = self.assembler.label();
        let protected_end = self.assembler.label();
        let cleanup = self.assembler.label();
        let parent_cleanup = self.control.active_comprehension_cleanups.last().copied();
        let cleanup_location = self.assembler.location();
        self.assembler.mark(protected_start);
        self.emit(build_opcode, 0, 1)?;
        self.emit(SWAP, 2, 0)?;
        let cleanup_depth =
            (base_depth + i32::try_from(temporary_names.len()).unwrap() + 1).cast_unsigned();
        let ((), region_exclusions) =
            self.with_comprehension_context((cleanup, cleanup_depth), |compiler| {
                compiler.with_control_override(
                    |control| &mut control.reorder_async_comprehension_cleanup_throw,
                    // CPython 3.14.5's flow graph leaves the throw cleanup immediately before
                    // `END_SEND` for a discarded async comprehension whose target is captured.
                    discard_result && !comprehension_cell_names.is_empty(),
                    |compiler| {
                        compiler
                            .compile_comprehension_generator(generators, 0, key, value, add_opcode)
                    },
                )
            })?;
        self.assembler.mark(protected_end);

        if temporary_names.is_empty() {
            self.set_depth(base_depth + 1);
            if discard_result {
                self.assembler
                    .set_location(self.source_location(generators.last().unwrap().iter.range()));
                self.emit(POP_TOP, 0, -1)?;
            }
            return Ok(());
        }

        let normal_cleanup = self.assembler.label();
        self.emit_jump_forward(JUMP_FORWARD, normal_cleanup, 0)?;
        if discard_result && generators.iter().any(|generator| generator.is_async) {
            self.assembler.prevent_last_jump_inlining();
        }
        self.assembler.mark(cleanup);
        self.set_depth(base_depth + i32::try_from(temporary_indices.len()).unwrap() + 2);
        self.assembler.set_location(SourceLocation::NONE);
        self.emit(SWAP, 2, 0)?;
        self.emit(POP_TOP, 0, -1)?;
        self.assembler.set_location(cleanup_location);
        self.emit(
            SWAP,
            to_u32(temporary_indices.len() + 1, "comprehension local count")?,
            0,
        )?;
        for (position, index) in temporary_indices.iter().rev().enumerate() {
            if position > 0 {
                self.assembler.fusion_barrier();
            }
            self.emit(STORE_FAST, *index, -1)?;
        }
        self.emit(RERAISE, 0, -1)?;
        let cleanup_end = self.assembler.label();
        self.assembler.mark(cleanup_end);
        if let Some((parent, depth)) = parent_cleanup {
            self.assembler
                .add_exception_region(cleanup, cleanup_end, parent, depth, false);
        }
        self.assembler.mark(normal_cleanup);

        let deferred_normal_restore = self.control.defer_async_comprehension_restore;
        self.set_depth(base_depth + i32::try_from(temporary_names.len()).unwrap() + 1);
        self.assembler.set_location(comprehension_location);
        if deferred_normal_restore {
            self.control
                .pending_comprehension_restores
                .push((temporary_indices.clone(), comprehension_location));
        } else {
            if discard_result {
                self.emit(POP_TOP, 0, -1)?;
            } else {
                self.emit(
                    SWAP,
                    to_u32(temporary_names.len() + 1, "comprehension local count")?,
                    0,
                )?;
            }
            if discard_result {
                let final_index = temporary_indices.last();
                let restore_indices = temporary_indices
                    [..temporary_indices.len().saturating_sub(1)]
                    .iter()
                    .rev()
                    .chain(final_index);
                for (position, index) in restore_indices.enumerate() {
                    if position > 0 {
                        self.assembler.fusion_barrier();
                    }
                    self.emit(STORE_FAST, *index, -1)?;
                    self.assembler
                        .prevent_last_instruction_fusion_with_previous();
                }
            } else {
                for (position, index) in temporary_indices.iter().rev().enumerate() {
                    if position > 0 {
                        self.assembler.fusion_barrier();
                    }
                    self.emit(STORE_FAST, *index, -1)?;
                    self.assembler
                        .prevent_last_instruction_fusion_with_previous();
                }
            }
            // CPython converts STORE_FAST_MAYBE_NULL only after inserting superinstructions.
            self.assembler.prevent_last_instruction_fusion();
        }
        for name in active_temporary_ids {
            self.symbols.active_temporaries.remove(name);
        }

        let mut region_start = protected_start;
        for (exclusion_start, exclusion_end) in region_exclusions {
            self.assembler.add_exception_region(
                region_start,
                exclusion_start,
                cleanup,
                cleanup_depth,
                false,
            );
            region_start = exclusion_end;
        }
        self.assembler.add_exception_region(
            region_start,
            protected_end,
            cleanup,
            cleanup_depth,
            false,
        );
        self.set_depth(
            base_depth
                + if deferred_normal_restore {
                    i32::try_from(temporary_names.len()).unwrap() + 1
                } else {
                    i32::from(!discard_result)
                },
        );
        Ok(())
    }

    fn compile_generator_expression(
        &mut self,
        generator: &ruff_python_ast::ExprGenerator,
    ) -> Result<(), CompileError> {
        if generator.generators.is_empty() {
            return Err(CompileError::Internal(
                "generator expression has no generators".to_string(),
            ));
        }
        let is_async = generator.generators.iter().any(|generator| {
            generator.is_async
                || expression_contains_await(&generator.iter)
                || generator.ifs.iter().any(expression_contains_await)
        }) || expression_contains_await(&generator.elt);

        let mut locals = vec![".0".to_string()];
        for comprehension in &generator.generators {
            collect_target_names(&comprehension.target, &mut locals);
        }
        let mut seen = HashSet::new();
        locals.retain(|name| seen.insert(name.clone()));
        let required_names = generator_required_names(generator);
        let generator_cellvars = generator_cellvars(generator);
        let mut globals = HashSet::default();
        let mut freevars = BTreeSet::new();
        for name in required_names {
            if self.can_provide_closure(&name) {
                freevars.insert(name);
            } else {
                globals.insert(name);
            }
        }
        let closure_names: Vec<_> = freevars.iter().cloned().collect();
        let generator_range = self.generator_expression_range(generator.range);
        let plan = LoweringFunctionPlan {
            locals,
            globals,
            cellvars: generator_cellvars,
            freevars,
            ..LoweringFunctionPlan::default()
        };
        let flags = if is_async {
            CO_ASYNC_GENERATOR
        } else {
            CO_GENERATOR
        } | if self.child_function_is_nested() {
            CO_NESTED
        } else {
            0
        } | if matches!(self.unit.scope, Scope::Class { .. }) {
            CO_METHOD
        } else {
            0
        } | (self.unit.flags & SUPPORTED_FUTURE_FLAGS);
        let mut child = Self::function(
            Arc::clone(&self.context),
            "<genexpr>",
            self.child_qualified_name("<genexpr>"),
            self.line_number(u32::from(generator_range.start())),
            &plan,
            self.unit.scope_id,
            1,
            0,
            0,
            flags,
        )?;
        child.unit.private_name.clone_from(&self.unit.private_name);
        child
            .symbols
            .imported_scope_names
            .clone_from(&self.symbols.imported_scope_names);
        // CPython only inserts `.<locals>` for function, async-function, and lambda parents.
        child.unit.child_qualified_name_parent = Some(child.unit.qualified_name.clone());
        child.emit_function_prologue()?;
        let generator_location = child.source_location(generator_range);
        child.assembler.set_location(generator_location);
        child.emit_owned_fast(0, 1)?;
        child.compile_generator_expression_loop(
            &generator.generators,
            0,
            &generator.elt,
            generator_location,
        )?;
        let child = child.finish()?;

        self.assembler
            .set_location(self.source_location(generator_range));
        if !closure_names.is_empty() {
            self.emit_closure_tuple(&closure_names)?;
        }
        let code = self.add_constant(Constant::Code(Box::new(child)))?;
        self.emit(LOAD_CONST, code, 1)?;
        self.emit(MAKE_FUNCTION, 0, 0)?;
        if !closure_names.is_empty() {
            self.emit_function_attribute(FunctionAttribute::CLOSURE)?;
        }
        if generator.generators[0].is_async {
            self.compile_expression(&generator.generators[0].iter)?;
        } else {
            self.compile_iterable_expression(&generator.generators[0].iter)?;
        }
        self.assembler
            .set_location(self.source_location(generator.generators[0].iter.range()));
        self.emit(
            if generator.generators[0].is_async {
                GET_AITER
            } else {
                GET_ITER
            },
            0,
            0,
        )?;
        self.assembler
            .set_location(self.source_location(generator_range));
        self.emit(CALL, 0, -1)
    }

    fn compile_generator_expression_loop(
        &mut self,
        generators: &[ruff_python_ast::Comprehension],
        index: usize,
        element: &Expr,
        generator_location: SourceLocation,
    ) -> Result<(), CompileError> {
        let generator = &generators[index];
        if generator.is_async {
            return self.compile_async_generator_expression_loop(
                generators,
                index,
                element,
                generator_location,
            );
        }
        let loop_depth = self.control.depth;
        let start = self.assembler.label();
        let cleanup = self.assembler.label();
        self.assembler.mark(start);
        self.assembler
            .set_location(self.source_location(generator.iter.range()));
        self.emit_jump_forward(FOR_ITER, cleanup, 1)?;
        self.compile_store_target(&generator.target)?;
        for condition in &generator.ifs {
            let accepted = self.assembler.label();
            self.compile_generator_filter(condition, element, accepted, start)?;
            self.assembler.mark(accepted);
        }
        if index + 1 < generators.len() {
            if generators[index + 1].is_async {
                self.compile_expression(&generators[index + 1].iter)?;
            } else {
                self.compile_iterable_expression(&generators[index + 1].iter)?;
            }
            self.assembler
                .set_location(self.source_location(generators[index + 1].iter.range()));
            self.emit(
                if generators[index + 1].is_async {
                    GET_AITER
                } else {
                    GET_ITER
                },
                0,
                0,
            )?;
            self.compile_generator_expression_loop(
                generators,
                index + 1,
                element,
                generator_location,
            )?;
            self.assembler
                .set_location(self.source_location(element.range()));
        } else {
            self.compile_expression(element)?;
            self.assembler
                .set_location(self.source_location(element.range()));
            if self.unit.flags & CO_ASYNC_GENERATOR != 0 {
                self.emit_unary_intrinsic(UnaryIntrinsic::ASYNC_GEN_WRAP)?;
            }
            self.emit(YIELD_VALUE, 0, 0)?;
            self.emit_resume(ResumeLocation::AFTER_YIELD, true)?;
            self.emit(POP_TOP, 0, -1)?;
        }
        self.emit_jump_backward(JUMP_BACKWARD, start, 0)?;
        self.assembler.mark(cleanup);
        self.set_depth(loop_depth + 1);
        self.assembler
            .set_location(self.source_location(generator.iter.range()));
        self.emit(END_FOR, 0, -1)?;
        self.emit(POP_ITER, 0, -1)?;
        Ok(())
    }

    fn compile_async_generator_expression_loop(
        &mut self,
        generators: &[ruff_python_ast::Comprehension],
        index: usize,
        element: &Expr,
        generator_location: SourceLocation,
    ) -> Result<(), CompileError> {
        let generator = &generators[index];
        let loop_depth = self.control.depth;
        let cleanup_location = generator_location;
        let start = self.assembler.label();
        let protected_start = self.assembler.label();
        let yielded = self.assembler.label();
        let yielded_end = self.assembler.label();
        let send = self.assembler.label();
        let send_end = self.assembler.label();
        let protected_end = self.assembler.label();
        let cleanup_throw = self.assembler.label();
        let async_cleanup = self.assembler.label();

        self.assembler.set_location(generator_location);
        self.assembler.mark(start);
        self.assembler.mark(protected_start);
        self.emit(GET_ANEXT, 0, 1)?;
        let none = self.add_constant(Constant::None)?;
        self.emit(LOAD_CONST, none, 1)?;
        self.assembler.mark(send);
        self.emit_jump_forward(SEND, send_end, 0)?;
        self.assembler.mark(yielded);
        self.emit(YIELD_VALUE, 1, 0)?;
        self.assembler.mark(yielded_end);
        self.emit_resume(ResumeLocation::AFTER_AWAIT, false)?;
        self.emit_jump_backward(JUMP_BACKWARD_NO_INTERRUPT, send, 0)?;
        self.assembler.mark(send_end);
        self.set_depth(loop_depth + 2);
        self.emit(END_SEND, 0, -1)?;
        self.assembler.mark(protected_end);
        self.compile_store_target(&generator.target)?;

        for condition in &generator.ifs {
            let accepted = self.assembler.label();
            self.compile_generator_filter(condition, element, accepted, start)?;
            self.assembler.mark(accepted);
        }
        if index + 1 < generators.len() {
            if generators[index + 1].is_async {
                self.compile_expression(&generators[index + 1].iter)?;
            } else {
                self.compile_iterable_expression(&generators[index + 1].iter)?;
            }
            self.assembler
                .set_location(self.source_location(generators[index + 1].iter.range()));
            self.emit(
                if generators[index + 1].is_async {
                    GET_AITER
                } else {
                    GET_ITER
                },
                0,
                0,
            )?;
            self.compile_generator_expression_loop(
                generators,
                index + 1,
                element,
                generator_location,
            )?;
            self.assembler
                .set_location(self.source_location(element.range()));
        } else {
            self.compile_expression(element)?;
            self.assembler
                .set_location(self.source_location(element.range()));
            self.emit_unary_intrinsic(UnaryIntrinsic::ASYNC_GEN_WRAP)?;
            self.emit(YIELD_VALUE, 0, 0)?;
            self.emit_resume(ResumeLocation::AFTER_YIELD, true)?;
            self.emit(POP_TOP, 0, -1)?;
        }
        self.emit_jump_backward(JUMP_BACKWARD, start, 0)?;
        self.assembler.set_location(cleanup_location);

        self.assembler.mark(cleanup_throw);
        self.set_depth(loop_depth + 3);
        self.emit(CLEANUP_THROW, 0, -1)?;
        let cleanup_throw_end = self.assembler.label();
        self.assembler.mark(cleanup_throw_end);
        self.emit_jump_backward(JUMP_BACKWARD_NO_INTERRUPT, send_end, 0)?;
        self.assembler.add_exception_region(
            yielded,
            yielded_end,
            cleanup_throw,
            (loop_depth + 2).cast_unsigned(),
            false,
        );
        self.assembler.mark(async_cleanup);
        self.control
            .generator_region_exclusions
            .push((cleanup_throw, async_cleanup));
        self.set_depth(loop_depth + 1);
        self.apply_stack_effect(-2)?;
        self.assembler.emit_backward(END_ASYNC_FOR, send_end);
        self.assembler.add_exception_region(
            protected_start,
            protected_end,
            async_cleanup,
            loop_depth.cast_unsigned(),
            false,
        );
        self.assembler.add_exception_region(
            cleanup_throw,
            cleanup_throw_end,
            async_cleanup,
            loop_depth.cast_unsigned(),
            false,
        );
        Ok(())
    }

    fn compile_generator_filter(
        &mut self,
        condition: &Expr,
        element: &Expr,
        accepted: Label,
        restart: Label,
    ) -> Result<(), CompileError> {
        if early_condition_truthiness(condition) == Some(true) {
            self.record_folded_value(condition)?;
            return Ok(());
        }
        if let Expr::BoolOp(boolean) = condition
            && boolean.op == BoolOp::And
        {
            let Some((last, leading)) = boolean.values.split_last() else {
                return Err(CompileError::Internal(
                    "boolean expression contains no values".to_string(),
                ));
            };
            for value in leading {
                let next = self.assembler.label();
                self.compile_generator_filter(value, element, next, restart)?;
                self.assembler.mark(next);
            }
            return self.compile_generator_filter(last, element, accepted, restart);
        }
        if let Expr::Compare(comparison) = condition
            && comparison.ops.len() == 1
            && matches!(comparison.comparators[0], Expr::NoneLiteral(_))
            && matches!(comparison.ops[0], CmpOp::Is | CmpOp::IsNot)
        {
            self.compile_expression(&comparison.left)?;
            self.add_constant(Constant::None)?;
            let exclusion_start = self.assembler.label();
            self.assembler.mark(exclusion_start);
            self.assembler
                .set_location(self.source_location(element.range()));
            self.emit_jump_forward(
                if comparison.ops[0] == CmpOp::Is {
                    POP_JUMP_IF_NONE
                } else {
                    POP_JUMP_IF_NOT_NONE
                },
                accepted,
                -1,
            )?;
            self.emit(NOT_TAKEN, 0, 0)?;
            self.emit_jump_backward(JUMP_BACKWARD, restart, 0)?;
            let exclusion_end = self.assembler.label();
            self.assembler.mark(exclusion_end);
            self.control
                .generator_region_exclusions
                .push((exclusion_start, exclusion_end));
            return Ok(());
        }
        self.compile_expression(condition)?;
        self.assembler
            .set_location(self.source_location(condition.range()));
        self.emit(TO_BOOL, 0, 0)?;
        let exclusion_start = self.assembler.label();
        self.assembler.mark(exclusion_start);
        self.assembler
            .set_location(self.source_location(element.range()));
        self.emit_jump_forward(POP_JUMP_IF_TRUE, accepted, -1)?;
        self.emit(NOT_TAKEN, 0, 0)?;
        self.emit_jump_backward(JUMP_BACKWARD, restart, 0)?;
        let exclusion_end = self.assembler.label();
        self.assembler.mark(exclusion_end);
        self.control
            .generator_region_exclusions
            .push((exclusion_start, exclusion_end));
        Ok(())
    }

    fn compile_comprehension_filter(
        &mut self,
        condition: &Expr,
        element_location: SourceLocation,
        accepted: Label,
        restart: Label,
    ) -> Result<(), CompileError> {
        if comprehension_filter_truthiness(condition) == Some(true) {
            self.record_folded_value(condition)?;
            if matches!(condition, Expr::UnaryOp(unary) if unary.op == UnaryOp::Not) {
                self.emit_folded_tuple_not_nops(condition)?;
            } else {
                self.assembler
                    .set_location(self.source_location(condition.range()));
                self.emit(NOP, 0, 0)?;
            }
            return Ok(());
        }
        if let Expr::BoolOp(boolean) = condition
            && boolean.op == BoolOp::And
        {
            let Some((last, leading)) = boolean.values.split_last() else {
                return Err(CompileError::Internal(
                    "boolean expression contains no values".to_string(),
                ));
            };
            for value in leading {
                let next = self.assembler.label();
                self.compile_comprehension_filter(value, element_location, next, restart)?;
                self.assembler.mark(next);
            }
            return self.compile_comprehension_filter(last, element_location, accepted, restart);
        }
        if let Expr::BoolOp(boolean) = condition
            && boolean.op == BoolOp::Or
        {
            let Some((last, leading)) = boolean.values.split_last() else {
                return Err(CompileError::Internal(
                    "boolean expression contains no values".to_string(),
                ));
            };
            for value in leading {
                self.compile_jump_if(value, true, accepted)?;
            }
            return self.compile_comprehension_filter(last, element_location, accepted, restart);
        }
        if let Expr::UnaryOp(unary) = condition
            && unary.op == UnaryOp::Not
        {
            self.compile_expression(&unary.operand)?;
            self.assembler
                .set_location(self.source_location(unary.operand.range()));
            self.emit(TO_BOOL, 0, 0)?;
            let exclusion_start = self.assembler.label();
            self.assembler.mark(exclusion_start);
            self.assembler.set_location(element_location);
            self.emit_jump_forward(POP_JUMP_IF_FALSE, accepted, -1)?;
            self.emit(NOT_TAKEN, 0, 0)?;
            self.emit_jump_backward(JUMP_BACKWARD, restart, 0)?;
            let exclusion_end = self.assembler.label();
            self.assembler.mark(exclusion_end);
            self.control
                .generator_region_exclusions
                .push((exclusion_start, exclusion_end));
            for exclusions in &mut self.control.active_comprehension_region_exclusions {
                exclusions.push((exclusion_start, exclusion_end));
            }
            return Ok(());
        }
        if let Expr::Compare(comparison) = condition
            && comparison.ops.len() > 1
        {
            let base_depth = self.control.depth;
            let cleanup = self.assembler.label();
            let residue = self.assembler.label();
            let comparison_location = self.source_location(comparison.range);
            self.compile_expression(&comparison.left)?;
            for (operator, comparator) in comparison
                .ops
                .iter()
                .zip(&comparison.comparators)
                .take(comparison.ops.len() - 1)
            {
                self.compile_expression(comparator)?;
                self.assembler.set_location(comparison_location);
                self.emit(SWAP, 2, 0)?;
                self.emit(COPY, 2, 1)?;
                let (opcode, argument) = comparison_operator_boolean(*operator);
                self.emit(opcode, argument, -1)?;
                self.emit_jump_forward(POP_JUMP_IF_FALSE, cleanup, -1)?;
                self.emit(NOT_TAKEN, 0, 0)?;
            }

            self.compile_expression(comparison.comparators.last().unwrap())?;
            self.assembler.set_location(comparison_location);
            let (opcode, argument) = comparison_operator_boolean(*comparison.ops.last().unwrap());
            self.emit(opcode, argument, -1)?;
            let terminal_exclusion_start = self.assembler.label();
            self.assembler.mark(terminal_exclusion_start);
            self.assembler.set_location(element_location);
            self.emit_jump_forward(POP_JUMP_IF_TRUE, residue, -1)?;
            self.emit(NOT_TAKEN, 0, 0)?;
            self.emit_jump_backward(JUMP_BACKWARD, restart, 0)?;
            let terminal_exclusion_end = self.assembler.label();
            self.assembler.mark(terminal_exclusion_end);

            self.assembler.mark(residue);
            self.assembler.set_location(comparison_location);
            self.emit_jump_forward(JUMP_FORWARD, accepted, 0)?;

            self.assembler.mark(cleanup);
            self.set_depth(base_depth + 1);
            self.emit(POP_TOP, 0, -1)?;
            let cleanup_exclusion_start = self.assembler.label();
            self.assembler.mark(cleanup_exclusion_start);
            self.assembler.set_location(element_location);
            self.emit_jump_backward(JUMP_BACKWARD, restart, 0)?;
            let cleanup_exclusion_end = self.assembler.label();
            self.assembler.mark(cleanup_exclusion_end);
            self.set_depth(base_depth);

            for (start, end) in [
                (terminal_exclusion_start, terminal_exclusion_end),
                (cleanup_exclusion_start, cleanup_exclusion_end),
            ] {
                self.control.generator_region_exclusions.push((start, end));
                for exclusions in &mut self.control.active_comprehension_region_exclusions {
                    exclusions.push((start, end));
                }
            }
            return Ok(());
        }
        if let Expr::Compare(comparison) = condition
            && comparison.ops.len() == 1
            && matches!(comparison.comparators[0], Expr::NoneLiteral(_))
            && matches!(comparison.ops[0], CmpOp::Is | CmpOp::IsNot)
        {
            self.compile_expression(&comparison.left)?;
            self.add_constant(Constant::None)?;
            let exclusion_start = self.assembler.label();
            self.assembler.mark(exclusion_start);
            self.assembler.set_location(element_location);
            self.emit_jump_forward(
                if comparison.ops[0] == CmpOp::Is {
                    POP_JUMP_IF_NONE
                } else {
                    POP_JUMP_IF_NOT_NONE
                },
                accepted,
                -1,
            )?;
            self.emit(NOT_TAKEN, 0, 0)?;
            self.emit_jump_backward(JUMP_BACKWARD, restart, 0)?;
            let exclusion_end = self.assembler.label();
            self.assembler.mark(exclusion_end);
            self.control
                .generator_region_exclusions
                .push((exclusion_start, exclusion_end));
            for exclusions in &mut self.control.active_comprehension_region_exclusions {
                exclusions.push((exclusion_start, exclusion_end));
            }
            return Ok(());
        }
        self.compile_expression(condition)?;
        self.assembler
            .set_location(self.source_location(condition.range()));
        self.emit(TO_BOOL, 0, 0)?;
        let exclusion_start = self.assembler.label();
        self.assembler.mark(exclusion_start);
        self.assembler.set_location(element_location);
        self.emit_jump_forward(POP_JUMP_IF_TRUE, accepted, -1)?;
        self.emit(NOT_TAKEN, 0, 0)?;
        self.emit_jump_backward(JUMP_BACKWARD, restart, 0)?;
        let exclusion_end = self.assembler.label();
        self.assembler.mark(exclusion_end);
        self.control
            .generator_region_exclusions
            .push((exclusion_start, exclusion_end));
        for exclusions in &mut self.control.active_comprehension_region_exclusions {
            exclusions.push((exclusion_start, exclusion_end));
        }
        Ok(())
    }

    fn compile_comprehension_generator(
        &mut self,
        generators: &[ruff_python_ast::Comprehension],
        index: usize,
        key: Option<&Expr>,
        value: &Expr,
        add_opcode: Opcode,
    ) -> Result<(), CompileError> {
        let generator = &generators[index];
        if generator.is_async {
            return self
                .compile_async_comprehension_generator(generators, index, key, value, add_opcode);
        }
        let loop_depth = self.control.depth;
        let start = self.assembler.label();
        let cleanup = self.assembler.label();
        self.assembler.mark(start);
        self.assembler
            .set_location(self.source_location(generator.iter.range()));
        self.emit_jump_forward(FOR_ITER, cleanup, 1)?;
        self.compile_store_target(&generator.target)?;

        for condition in &generator.ifs {
            let accepted = self.assembler.label();
            let element_range = key.map_or_else(
                || value.range(),
                |key| ruff_text_size::TextRange::new(key.range().start(), value.range().end()),
            );
            self.compile_comprehension_filter(
                condition,
                self.source_location(element_range),
                accepted,
                start,
            )?;
            self.assembler.mark(accepted);
        }

        let mut loop_back_location = None;
        if index + 1 < generators.len() {
            let nested = &generators[index + 1];
            let singleton = match &nested.iter {
                Expr::List(list) if list.elts.len() == 1 => Some(&list.elts[0]),
                Expr::Tuple(tuple) if tuple.elts.len() == 1 => Some(&tuple.elts[0]),
                _ => None,
            };
            if index + 2 == generators.len()
                && nested.ifs.is_empty()
                && !nested.is_async
                && let Some(singleton) = singleton
            {
                self.compile_expression(singleton)?;
                self.compile_store_target(&nested.target)?;
                self.compile_comprehension_element(key, value)?;
                let add_range = key.map_or_else(
                    || value.range(),
                    |key| ruff_text_size::TextRange::new(key.range().start(), value.range().end()),
                );
                self.assembler.set_location(self.source_location(add_range));
                let effect = if key.is_some() { -2 } else { -1 };
                self.emit(
                    add_opcode,
                    to_u32(index + 2, "comprehension depth")?,
                    effect,
                )?;
                loop_back_location =
                    Some(self.source_location(key.map_or_else(|| value.range(), Ranged::range)));
            } else {
                if nested.is_async {
                    self.compile_expression(&nested.iter)?;
                } else {
                    self.compile_iterable_expression(&nested.iter)?;
                }
                self.assembler
                    .set_location(self.source_location(nested.iter.range()));
                self.emit(if nested.is_async { GET_AITER } else { GET_ITER }, 0, 0)?;
                self.compile_comprehension_generator(
                    generators,
                    index + 1,
                    key,
                    value,
                    add_opcode,
                )?;
                loop_back_location = Some(self.source_location(key.unwrap_or(value).range()));
            }
        } else {
            self.compile_comprehension_element(key, value)?;
            let add_range = key.map_or_else(
                || value.range(),
                |key| ruff_text_size::TextRange::new(key.range().start(), value.range().end()),
            );
            self.assembler.set_location(self.source_location(add_range));
            let effect = if key.is_some() { -2 } else { -1 };
            self.emit(
                add_opcode,
                to_u32(generators.len() + 1, "comprehension depth")?,
                effect,
            )?;
        }
        if let Some(location) = loop_back_location {
            self.assembler.set_location(location);
        } else if let Some(location) = self.assembler.last_instruction_location() {
            self.assembler.set_location(location);
        }
        self.emit_jump_backward(JUMP_BACKWARD, start, 0)?;
        self.assembler.mark(cleanup);
        self.set_depth(loop_depth + 1);
        self.assembler
            .set_location(self.source_location(generator.iter.range()));
        self.emit(END_FOR, 0, -1)?;
        self.emit(POP_ITER, 0, -1)?;
        Ok(())
    }

    fn compile_comprehension_element(
        &mut self,
        key: Option<&Expr>,
        value: &Expr,
    ) -> Result<(), CompileError> {
        let folded_conditional_key = matches!(key, Some(Expr::If(expression)) if early_condition_truthiness(&expression.test).is_some());
        self.compile_with_strict_owned_loads(folded_conditional_key, |compiler| {
            if let Some(key) = key {
                if folded_conditional_key {
                    // CPython retains the folded conditional's entry and join boundaries. They
                    // prevent fusion around the key and force both MAP_ADD operands to be owned.
                    compiler.assembler.fusion_barrier();
                }
                compiler.compile_expression(key)?;
                if folded_conditional_key {
                    compiler.assembler.fusion_barrier();
                }
            }
            compiler.compile_expression(value)
        })
    }

    fn compile_async_comprehension_generator(
        &mut self,
        generators: &[ruff_python_ast::Comprehension],
        index: usize,
        key: Option<&Expr>,
        value: &Expr,
        add_opcode: Opcode,
    ) -> Result<(), CompileError> {
        let generator = &generators[index];
        let reorder_cleanup = self.control.reorder_async_comprehension_cleanup_throw;
        let loop_depth = self.control.depth;
        let cleanup_location = self.assembler.location();
        let start = self.assembler.label();
        let protected_start = self.assembler.label();
        let yielded = self.assembler.label();
        let yielded_end = self.assembler.label();
        let send = self.assembler.label();
        let send_end = self.assembler.label();
        let protected_before_cleanup_end = self.assembler.label();
        let protected_after_cleanup_start = self.assembler.label();
        let protected_end = self.assembler.label();
        let cleanup_throw = self.assembler.label();
        let cleanup_throw_end = self.assembler.label();
        let async_cleanup = self.assembler.label();

        self.assembler.mark(start);
        self.assembler.mark(protected_start);
        self.emit(GET_ANEXT, 0, 1)?;
        let none = self.add_constant(Constant::None)?;
        self.emit(LOAD_CONST, none, 1)?;
        self.assembler.mark(send);
        self.emit_jump_forward(SEND, send_end, 0)?;
        self.assembler.mark(yielded);
        self.emit(YIELD_VALUE, 1, 0)?;
        self.assembler.mark(yielded_end);
        self.emit_resume(ResumeLocation::AFTER_AWAIT, false)?;
        self.emit_jump_backward(JUMP_BACKWARD_NO_INTERRUPT, send, 0)?;
        if reorder_cleanup {
            self.assembler.mark(protected_before_cleanup_end);
            self.assembler.set_location(cleanup_location);
            self.assembler.mark(cleanup_throw);
            self.set_depth(loop_depth + 3);
            self.emit(CLEANUP_THROW, 0, -1)?;
            self.assembler.mark(cleanup_throw_end);
            self.assembler.mark(protected_after_cleanup_start);
        }
        self.assembler.mark(send_end);
        self.set_depth(loop_depth + 2);
        self.emit(END_SEND, 0, -1)?;
        self.assembler.mark(protected_end);
        self.compile_store_target(&generator.target)?;

        for condition in &generator.ifs {
            let accepted = self.assembler.label();
            let element_range = key.map_or_else(
                || value.range(),
                |key| ruff_text_size::TextRange::new(key.range().start(), value.range().end()),
            );
            self.compile_comprehension_filter(
                condition,
                self.source_location(element_range),
                accepted,
                start,
            )?;
            self.assembler.mark(accepted);
        }
        if index + 1 < generators.len() {
            if generators[index + 1].is_async {
                self.compile_expression(&generators[index + 1].iter)?;
            } else {
                self.compile_iterable_expression(&generators[index + 1].iter)?;
            }
            self.assembler
                .set_location(self.source_location(generators[index + 1].iter.range()));
            self.emit(
                if generators[index + 1].is_async {
                    GET_AITER
                } else {
                    GET_ITER
                },
                0,
                0,
            )?;
            self.compile_comprehension_generator(generators, index + 1, key, value, add_opcode)?;
            self.assembler
                .set_location(self.source_location(key.unwrap_or(value).range()));
        } else {
            self.compile_comprehension_element(key, value)?;
            let add_range = key.map_or_else(
                || value.range(),
                |key| ruff_text_size::TextRange::new(key.range().start(), value.range().end()),
            );
            self.assembler.set_location(self.source_location(add_range));
            let effect = if key.is_some() { -2 } else { -1 };
            self.emit(
                add_opcode,
                to_u32(generators.len() + 1, "comprehension depth")?,
                effect,
            )?;
        }
        self.emit_jump_backward(JUMP_BACKWARD, start, 0)?;
        self.assembler.set_location(cleanup_location);
        if !reorder_cleanup {
            self.assembler.mark(cleanup_throw);
            self.set_depth(loop_depth + 3);
            self.emit(CLEANUP_THROW, 0, -1)?;
            self.assembler.mark(cleanup_throw_end);
            self.emit_jump_backward(JUMP_BACKWARD_NO_INTERRUPT, send_end, 0)?;
        }
        self.assembler.add_exception_region(
            yielded,
            yielded_end,
            cleanup_throw,
            (loop_depth + 2).cast_unsigned(),
            false,
        );
        self.assembler.mark(async_cleanup);
        self.set_depth(loop_depth + 1);
        self.apply_stack_effect(-2)?;
        self.assembler.emit_backward(
            END_ASYNC_FOR,
            if reorder_cleanup {
                cleanup_throw
            } else {
                send_end
            },
        );
        if reorder_cleanup {
            let async_cleanup_end = self.assembler.label();
            self.assembler.mark(async_cleanup_end);
            self.control
                .generator_region_exclusions
                .push((cleanup_throw, async_cleanup_end));
            self.assembler.add_exception_region(
                protected_start,
                protected_before_cleanup_end,
                async_cleanup,
                loop_depth.cast_unsigned(),
                false,
            );
            self.assembler.add_exception_region(
                protected_after_cleanup_start,
                protected_end,
                async_cleanup,
                loop_depth.cast_unsigned(),
                false,
            );
        } else {
            self.control
                .generator_region_exclusions
                .push((cleanup_throw, async_cleanup));
            for exclusions in &mut self.control.active_comprehension_region_exclusions {
                exclusions.push((cleanup_throw_end, async_cleanup));
            }
            for exclusions in &mut self.control.active_with_region_exclusions {
                exclusions.push((cleanup_throw_end, async_cleanup));
            }
            for exclusions in &mut self.control.active_exception_region_exclusions {
                exclusions.push((cleanup_throw_end, async_cleanup));
            }
            self.assembler.add_exception_region(
                protected_start,
                protected_end,
                async_cleanup,
                loop_depth.cast_unsigned(),
                false,
            );
        }
        self.assembler.add_exception_region(
            cleanup_throw,
            cleanup_throw_end,
            async_cleanup,
            loop_depth.cast_unsigned(),
            false,
        );
        Ok(())
    }

    fn ensure_temporary(&mut self, name: &str) -> Result<u32, CompileError> {
        if let Some(index) = self.temporary_index(name) {
            return Ok(index);
        }
        let name_id = self.required_name_id(name);
        let existing = match &self.unit.scope {
            Scope::Function { indices, .. } => indices.get(&name_id).copied(),
            Scope::Module | Scope::Class { .. } => None,
        };
        let index = if let Some(index) = existing {
            index
        } else {
            let index = to_u32(self.symbols.locals.len(), "comprehension local count")?;
            self.symbols.locals.push(name.to_string());
            if !matches!(self.unit.scope, Scope::Function { .. }) {
                self.symbols.hidden_names.insert(name_id);
            }
            index
        };
        self.symbols.temporary_indices.insert(name_id, index);
        Ok(index)
    }

    fn compile_collection(
        &mut self,
        elements: &[Expr],
        kind: CollectionKind,
    ) -> Result<(), CompileError> {
        if !elements.iter().any(Expr::is_starred_expr) {
            if kind == CollectionKind::Tuple
                && let Some(constants) = elements
                    .iter()
                    .map(fold_constant)
                    .collect::<Option<Vec<_>>>()
            {
                for element in elements {
                    self.record_folded_value(element)?;
                }
                return self.emit_deferred_constant(Constant::Tuple(constants));
            }
            if elements.len() > 30 {
                self.emit(kind.build_opcode_for_unpacking(), 0, 1)?;
                for element in elements {
                    self.compile_expression(element)?;
                    self.emit(kind.append_opcode(), 1, -1)?;
                }
                if kind == CollectionKind::Tuple {
                    self.emit_unary_intrinsic(UnaryIntrinsic::LIST_TO_TUPLE)?;
                }
                return Ok(());
            }
            if matches!(kind, CollectionKind::List | CollectionKind::Set)
                && elements.len() >= 3
                && let Some(constants) = elements
                    .iter()
                    .map(fold_constant)
                    .collect::<Option<Vec<_>>>()
            {
                for element in elements {
                    self.record_folded_value(element)?;
                }
                let constant = if kind == CollectionKind::Set {
                    let mut unique = Vec::with_capacity(constants.len());
                    for constant in constants {
                        if !unique
                            .iter()
                            .any(|existing| python_constants_equal(existing, &constant))
                        {
                            unique.push(constant);
                        }
                    }
                    Constant::FrozenSet(unique)
                } else {
                    Constant::Tuple(constants)
                };
                self.emit(kind.build_opcode_for_unpacking(), 0, 1)?;
                self.emit_deferred_constant(constant)?;
                self.emit(
                    if kind == CollectionKind::List {
                        LIST_EXTEND
                    } else {
                        SET_UPDATE
                    },
                    1,
                    -1,
                )?;
                return Ok(());
            }
            for element in elements {
                self.compile_expression(element)?;
            }
            return self.emit_build(kind.build_opcode(), elements.len());
        }

        let unpacking_start = elements
            .iter()
            .position(Expr::is_starred_expr)
            .unwrap_or(elements.len());
        if unpacking_start >= 3
            && let Some(constants) = elements[..unpacking_start]
                .iter()
                .map(fold_constant)
                .collect::<Option<Vec<_>>>()
        {
            for element in &elements[..unpacking_start] {
                self.record_folded_value(element)?;
            }
            let constant = if kind == CollectionKind::Set {
                let mut unique = Vec::with_capacity(constants.len());
                for constant in constants {
                    if !unique
                        .iter()
                        .any(|existing| python_constants_equal(existing, &constant))
                    {
                        unique.push(constant);
                    }
                }
                Constant::FrozenSet(unique)
            } else {
                Constant::Tuple(constants)
            };
            self.emit(kind.build_opcode_for_unpacking(), 0, 1)?;
            self.emit_deferred_constant(constant)?;
            self.emit(kind.extend_opcode(), 1, -1)?;
        } else {
            for element in &elements[..unpacking_start] {
                self.compile_expression(element)?;
            }
            self.emit_build(kind.build_opcode_for_unpacking(), unpacking_start)?;
        }
        for element in &elements[unpacking_start..] {
            if let Expr::Starred(starred) = element {
                self.compile_expression(&starred.value)?;
                self.emit(kind.extend_opcode(), 1, -1)?;
            } else {
                self.compile_expression(element)?;
                self.emit(kind.append_opcode(), 1, -1)?;
            }
        }
        if kind == CollectionKind::Tuple {
            self.emit_unary_intrinsic(UnaryIntrinsic::LIST_TO_TUPLE)?;
        }
        Ok(())
    }

    pub(super) fn compile_iterable_expression(
        &mut self,
        expression: &Expr,
    ) -> Result<(), CompileError> {
        let previous = self.assembler.location();
        self.assembler
            .set_location(self.source_location(expression.range()));
        let result = match expression {
            Expr::Tuple(tuple)
                if tuple.elts.iter().any(Expr::is_starred_expr) || tuple.elts.len() > 30 =>
            {
                // CPython lowers these tuples through a temporary list, then removes the
                // list-to-tuple intrinsic when the tuple is immediately iterated.
                self.compile_collection(&tuple.elts, CollectionKind::List)
            }
            Expr::List(list) if !list.elts.iter().any(Expr::is_starred_expr) => {
                if let Some(constants) = list
                    .elts
                    .iter()
                    .map(fold_constant)
                    .collect::<Option<Vec<_>>>()
                {
                    for element in &list.elts {
                        self.record_folded_value(element)?;
                    }
                    self.emit_deferred_constant(Constant::Tuple(constants))
                } else {
                    self.compile_collection(&list.elts, CollectionKind::Tuple)
                }
            }
            Expr::Set(set)
                if !set.elts.iter().any(Expr::is_starred_expr)
                    && set
                        .elts
                        .iter()
                        .all(|element| fold_constant(element).is_some()) =>
            {
                let mut unique = Vec::with_capacity(set.elts.len());
                for element in &set.elts {
                    self.record_folded_value(element)?;
                    let constant = fold_constant(element).unwrap();
                    if !unique
                        .iter()
                        .any(|existing| python_constants_equal(existing, &constant))
                    {
                        unique.push(constant);
                    }
                }
                self.emit_deferred_constant(Constant::FrozenSet(unique))
            }
            _ => self.compile_expression_inner(expression),
        };
        self.assembler.set_location(previous);
        result
    }

    fn compile_dict(&mut self, items: &[ruff_python_ast::DictItem]) -> Result<(), CompileError> {
        if items.iter().all(|item| item.key.is_some()) {
            if items.len() > 15 {
                self.emit(BUILD_MAP, 0, 1)?;
                let mut index = 0;
                let mut first_group = true;
                while items.len() - index > 15 {
                    if !first_group {
                        self.emit(BUILD_MAP, 0, 1)?;
                    }
                    let group_end = (index + 17).min(items.len());
                    while index < group_end {
                        self.compile_expression(items[index].key.as_ref().unwrap())?;
                        self.compile_expression(&items[index].value)?;
                        self.emit(MAP_ADD, 1, -2)?;
                        index += 1;
                    }
                    if !first_group {
                        self.emit(DICT_UPDATE, 1, -1)?;
                    }
                    first_group = false;
                }
                if index < items.len() {
                    let tail_start = index;
                    while index < items.len() {
                        self.compile_expression(items[index].key.as_ref().unwrap())?;
                        self.compile_expression(&items[index].value)?;
                        index += 1;
                    }
                    self.emit_build_map(index - tail_start)?;
                    self.emit(DICT_UPDATE, 1, -1)?;
                }
                return Ok(());
            }
            for item in items {
                self.compile_expression(item.key.as_ref().unwrap())?;
                self.compile_expression(&item.value)?;
            }
            let length = to_u32(items.len(), "dictionary item count")?;
            return self.emit(BUILD_MAP, length, 1 - i32::try_from(length).unwrap() * 2);
        }

        let mut index = 0;
        while index < items.len() && items[index].key.is_some() {
            self.compile_expression(items[index].key.as_ref().unwrap())?;
            self.compile_expression(&items[index].value)?;
            index += 1;
        }
        self.emit_build_map(index)?;

        while index < items.len() {
            self.compile_expression(&items[index].value)?;
            self.emit(DICT_UPDATE, 1, -1)?;
            index += 1;

            let group_start = index;
            while index < items.len() && items[index].key.is_some() {
                self.compile_expression(items[index].key.as_ref().unwrap())?;
                self.compile_expression(&items[index].value)?;
                index += 1;
            }
            if index > group_start {
                self.emit_build_map(index - group_start)?;
                self.emit(DICT_UPDATE, 1, -1)?;
            }
        }
        Ok(())
    }

    pub(super) fn emit_build_map(&mut self, length: usize) -> Result<(), CompileError> {
        let length = to_u32(length, "dictionary item count")?;
        self.emit(BUILD_MAP, length, 1 - i32::try_from(length).unwrap() * 2)
    }

    fn compile_slice(&mut self, slice: &ruff_python_ast::ExprSlice) -> Result<(), CompileError> {
        let slice_location = self.source_location(slice.range);
        if let Some(constant) = constant_slice(slice) {
            let index = self.add_constant(constant)?;
            self.assembler.set_location(slice_location);
            return self.emit(LOAD_CONST, index, 1);
        }
        for part in [&slice.lower, &slice.upper] {
            self.compile_optional_slice_bound(part.as_deref(), slice_location)?;
        }
        let argument_count = if let Some(step) = &slice.step {
            self.compile_expression(step)?;
            3
        } else {
            2
        };
        self.emit(
            BUILD_SLICE,
            argument_count,
            1 - argument_count.cast_signed(),
        )
    }

    pub(super) fn compile_optional_slice_bound(
        &mut self,
        bound: Option<&Expr>,
        fallback_location: SourceLocation,
    ) -> Result<(), CompileError> {
        if let Some(bound) = bound {
            self.compile_expression(bound)
        } else {
            let none = self.add_constant(Constant::None)?;
            self.assembler.set_location(fallback_location);
            self.emit(LOAD_CONST, none, 1)
        }
    }

    fn compile_if_expression(
        &mut self,
        expression: &ruff_python_ast::ExprIf,
    ) -> Result<(), CompileError> {
        if let Some(truthiness) = early_condition_truthiness(&expression.test) {
            if let Some(constant) = fold_constant(&expression.test)
                && (self.output.constants.is_empty() || matches!(constant, Constant::None))
            {
                self.add_constant(constant)?;
            }
            self.pre_register_expression_names(&expression.body)?;
            self.pre_register_expression_names(&expression.orelse)?;
            let selected = if truthiness {
                &expression.body
            } else {
                &expression.orelse
            };
            if self.source_location(expression.test.range()).line
                != self.source_location(selected.range()).line
            {
                // Folding a multiline condition leaves its source-position marker in CPython's
                // optimized flow graph.
                self.assembler
                    .set_location(self.source_location(expression.test.range()));
                self.emit(NOP, 0, 0)?;
            }
            self.compile_expression(selected)?;
            // CPython leaves the eliminated branch as an empty CFG block before the join.
            // Stack-depth analysis crosses that block, but borrowed-load analysis does not,
            // so loads emitted after the folded conditional remain owned.
            self.assembler.prevent_next_borrow_reachability();
            return Ok(());
        }
        let base_depth = self.control.depth;
        let else_label = self.assembler.label();
        let end = self.assembler.label();
        self.compile_jump_if(&expression.test, false, else_label)?;
        self.compile_expression(&expression.body)?;
        self.assembler
            .set_location(self.source_location(expression.body.range()));
        self.emit_jump_forward(JUMP_FORWARD, end, 0)?;
        self.assembler.mark(else_label);
        self.set_depth(base_depth);
        self.compile_expression(&expression.orelse)?;
        self.assembler.preserve_block_boundary(end);
        self.assembler.mark(end);
        self.set_depth(base_depth + 1);
        Ok(())
    }

    fn compile_bool_expression(
        &mut self,
        expression: &ruff_python_ast::ExprBoolOp,
    ) -> Result<(), CompileError> {
        self.compile_bool_expression_at(expression, self.source_location(expression.range))
    }

    fn compile_bool_expression_at(
        &mut self,
        expression: &ruff_python_ast::ExprBoolOp,
        location: SourceLocation,
    ) -> Result<(), CompileError> {
        let mut effective = Vec::with_capacity(expression.values.len());
        let mut short_circuited = false;
        let mut folded_prefix = false;
        for (index, value) in expression.values.iter().enumerate() {
            let is_last = index + 1 == expression.values.len();
            if short_circuited {
                self.pre_register_expression_names(value)?;
                if fold_constant(value).is_some() {
                    self.record_folded_value(value)?;
                }
                continue;
            }
            if !is_last && let Some(truthiness) = early_condition_truthiness(value) {
                let decisive = matches!(expression.op, BoolOp::And) && !truthiness
                    || matches!(expression.op, BoolOp::Or) && truthiness;
                if decisive {
                    effective.push(value);
                    short_circuited = true;
                } else {
                    self.record_folded_value(value)?;
                    folded_prefix |= index == 0 && fold_constant(value).is_some();
                }
                continue;
            }
            effective.push(value);
        }

        let Some((last, leading)) = effective.split_last() else {
            return Err(CompileError::Internal(
                "boolean expression contains no values".to_string(),
            ));
        };
        self.assembler.set_location(location);
        if folded_prefix {
            // CPython folds the leading constant load and its boolean jump, but retains the
            // jump as a source-position NOP when the next operand starts on another line.
            self.emit(NOP, 0, 0)?;
        }
        let base_depth = self.control.depth;
        let end = self.assembler.label();
        let jump = match expression.op {
            BoolOp::And => POP_JUMP_IF_FALSE,
            BoolOp::Or => POP_JUMP_IF_TRUE,
        };

        for value in leading {
            if let Expr::BoolOp(inner) = value
                && inner.op == expression.op
            {
                self.compile_bool_expression_at(inner, location)?;
            } else {
                self.compile_expression(value)?;
            }
            self.assembler.set_location(location);
            let exclusion_start = self.assembler.label();
            self.assembler.mark(exclusion_start);
            self.emit(COPY, 1, 1)?;
            self.emit(TO_BOOL, 0, 0)?;
            let exclusion_end = self.assembler.label();
            self.assembler.mark(exclusion_end);
            self.emit_jump_forward(jump, end, -1)?;
            self.emit(NOT_TAKEN, 0, 0)?;
            self.assembler
                .set_location(self.source_location(expression.range));
            self.emit(POP_TOP, 0, -1)?;
        }
        if let Expr::BoolOp(inner) = last
            && inner.op == expression.op
        {
            self.compile_bool_expression_at(inner, location)?;
        } else {
            self.compile_expression(last)?;
        }
        self.assembler.preserve_block_boundary(end);
        self.assembler.mark(end);
        self.set_depth(base_depth + 1);
        Ok(())
    }

    fn compile_compare_expression(
        &mut self,
        expression: &ruff_python_ast::ExprCompare,
    ) -> Result<(), CompileError> {
        self.compile_expression(&expression.left)?;
        if expression.ops.len() == 1 {
            self.compile_comparison_rhs(expression.ops[0], &expression.comparators[0])?;
            let (opcode, argument) = comparison_operator(expression.ops[0]);
            return self.emit(opcode, argument, -1);
        }

        let base_depth = self.control.depth - 1;
        let cleanup = self.assembler.label();
        for (operator, comparator) in expression
            .ops
            .iter()
            .zip(&expression.comparators)
            .take(expression.ops.len() - 1)
        {
            self.compile_expression(comparator)?;
            self.emit(SWAP, 2, 0)?;
            self.emit(COPY, 2, 1)?;
            let (opcode, argument) = comparison_operator(*operator);
            self.emit(opcode, argument, -1)?;
            self.emit(COPY, 1, 1)?;
            self.emit(TO_BOOL, 0, 0)?;
            self.emit_jump_forward(POP_JUMP_IF_FALSE, cleanup, -1)?;
            self.emit(NOT_TAKEN, 0, 0)?;
            self.emit(POP_TOP, 0, -1)?;
        }
        self.compile_expression(expression.comparators.last().unwrap())?;
        let (opcode, argument) = comparison_operator(*expression.ops.last().unwrap());
        self.emit(opcode, argument, -1)?;
        let end = self.assembler.label();
        self.emit_jump_forward(JUMP_FORWARD, end, 0)?;

        self.assembler.mark(cleanup);
        self.set_depth(base_depth + 2);
        self.emit(SWAP, 2, 0)?;
        self.emit(POP_TOP, 0, -1)?;
        self.assembler.mark(end);
        self.set_depth(base_depth + 1);
        Ok(())
    }

    fn compile_comparison_rhs(
        &mut self,
        operator: CmpOp,
        expression: &Expr,
    ) -> Result<(), CompileError> {
        if matches!(operator, CmpOp::In | CmpOp::NotIn) {
            self.compile_iterable_expression(expression)
        } else {
            self.compile_expression(expression)
        }
    }

    pub(super) fn compile_jump_if(
        &mut self,
        expression: &Expr,
        jump_on: bool,
        label: Label,
    ) -> Result<(), CompileError> {
        if let Expr::UnaryOp(unary) = expression
            && unary.op == UnaryOp::Not
        {
            return self.compile_jump_if(&unary.operand, !jump_on, label);
        }
        if let Expr::If(conditional) = expression
            && let Some(truthiness) = early_condition_truthiness(&conditional.test)
        {
            if let Some(constant) = fold_constant(&conditional.test)
                && (self.output.constants.is_empty() || matches!(constant, Constant::None))
            {
                self.add_constant(constant)?;
            }
            self.pre_register_expression_names(&conditional.body)?;
            self.pre_register_expression_names(&conditional.orelse)?;
            return self.compile_jump_if(
                if truthiness {
                    &conditional.body
                } else {
                    &conditional.orelse
                },
                jump_on,
                label,
            );
        }
        let constant_truthiness = jump_constant_truthiness(expression);
        if let Some(truthiness) = constant_truthiness {
            self.record_folded_value(expression)?;
            self.assembler
                .set_location(self.source_location(expression.range()));
            if truthiness == jump_on {
                self.emit_jump_forward(JUMP_FORWARD, label, 0)?;
                self.assembler.preserve_last_inlined_jump_nop();
            } else {
                let exclusion_start = self.assembler.label();
                self.assembler.mark(exclusion_start);
                self.emit(NOP, 0, 0)?;
                let exclusion_end = self.assembler.label();
                self.assembler.mark(exclusion_end);
                if self.control.generator_region_start.is_some() {
                    self.control
                        .generator_region_exclusions
                        .push((exclusion_start, exclusion_end));
                }
                for exclusions in &mut self.control.active_with_region_exclusions {
                    exclusions.push((exclusion_start, exclusion_end));
                }
                for exclusions in &mut self.control.active_exception_region_exclusions {
                    exclusions.push((exclusion_start, exclusion_end));
                }
            }
            return Ok(());
        }
        if let Expr::BoolOp(boolean) = expression {
            let Some((last, leading)) = boolean.values.split_last() else {
                return Err(CompileError::Internal(
                    "boolean expression contains no values".to_string(),
                ));
            };
            let short_circuit_jumps_to_target = matches!(boolean.op, BoolOp::Or) == jump_on;
            if short_circuit_jumps_to_target {
                for value in leading {
                    self.compile_jump_if(value, jump_on, label)?;
                }
                return self.compile_jump_if(last, jump_on, label);
            }
            let end = self.assembler.label();
            for value in leading {
                self.compile_jump_if(value, !jump_on, end)?;
            }
            self.compile_jump_if(last, jump_on, label)?;
            self.assembler.preserve_block_boundary(end);
            self.assembler.mark(end);
            return Ok(());
        }
        if let Expr::If(conditional) = expression {
            let otherwise = self.assembler.label();
            let end = self.assembler.label();
            self.compile_jump_if(&conditional.test, false, otherwise)?;
            self.compile_jump_if(&conditional.body, jump_on, label)?;
            self.assembler.set_location(SourceLocation::NONE);
            self.emit_jump_forward(JUMP_FORWARD, end, 0)?;
            self.assembler.mark(otherwise);
            self.compile_jump_if(&conditional.orelse, jump_on, label)?;
            self.assembler.preserve_block_boundary(end);
            self.assembler.mark(end);
            return Ok(());
        }
        if let Expr::Compare(comparison) = expression
            && comparison.ops.len() > 1
        {
            let base_depth = self.control.depth;
            let cleanup = self.assembler.label();
            let end = self.assembler.label();
            self.compile_expression(&comparison.left)?;
            for (operator, comparator) in comparison
                .ops
                .iter()
                .zip(&comparison.comparators)
                .take(comparison.ops.len() - 1)
            {
                self.compile_expression(comparator)?;
                self.assembler
                    .set_location(self.source_location(comparison.range));
                self.emit(SWAP, 2, 0)?;
                self.emit(COPY, 2, 1)?;
                let (opcode, argument) = comparison_operator_boolean(*operator);
                self.emit(opcode, argument, -1)?;
                self.emit_jump_forward(POP_JUMP_IF_FALSE, cleanup, -1)?;
                self.emit(NOT_TAKEN, 0, 0)?;
            }

            self.compile_expression(comparison.comparators.last().unwrap())?;
            self.assembler
                .set_location(self.source_location(comparison.range));
            let (opcode, argument) = comparison_operator_boolean(*comparison.ops.last().unwrap());
            self.emit(opcode, argument, -1)?;
            self.emit_jump_forward(
                if jump_on {
                    POP_JUMP_IF_TRUE
                } else {
                    POP_JUMP_IF_FALSE
                },
                label,
                -1,
            )?;
            self.emit(NOT_TAKEN, 0, 0)?;
            self.emit_jump_forward(JUMP_FORWARD, end, 0)?;
            if !jump_on {
                self.assembler.prevent_last_jump_inlining();
            }

            self.assembler.mark(cleanup);
            self.set_depth(base_depth + 1);
            self.emit(POP_TOP, 0, -1)?;
            if !jump_on {
                self.emit_jump_forward(JUMP_FORWARD, label, 0)?;
            }

            self.assembler.mark(end);
            self.set_depth(base_depth);
            return Ok(());
        }
        if let Expr::Compare(comparison) = expression
            && comparison.ops.len() == 1
            && matches!(comparison.comparators[0], Expr::NoneLiteral(_))
            && matches!(comparison.ops[0], CmpOp::Is | CmpOp::IsNot)
        {
            self.compile_expression(&comparison.left)?;
            self.add_constant(Constant::None)?;
            let left_location = self.source_location(comparison.left.range());
            let none_location = self.source_location(comparison.comparators[0].range());
            if left_location.line != none_location.line {
                self.assembler.set_location(none_location);
                self.emit(NOP, 0, 0)?;
            }
            self.assembler
                .set_location(self.source_location(expression.range()));
            let jump_if_none = jump_on == (comparison.ops[0] == CmpOp::Is);
            self.emit_jump_forward(
                if jump_if_none {
                    POP_JUMP_IF_NONE
                } else {
                    POP_JUMP_IF_NOT_NONE
                },
                label,
                -1,
            )?;
            if self.control.exclude_terminal_if_not_taken
                || self.control.exclude_condition_not_taken_from_exception
            {
                let exclude_from_generator =
                    self.control.exclude_condition_not_taken_from_exception
                        && self.control.generator_region_start.is_some();
                let exclude_from_exception =
                    self.control.exclude_condition_not_taken_from_exception
                        || (self.control.exclude_terminal_if_not_taken
                            && !self.control.active_with_region_exclusions.is_empty());
                self.control.exclude_terminal_if_not_taken = false;
                let exclusion_start = self.assembler.label();
                self.assembler.mark(exclusion_start);
                self.emit(NOT_TAKEN, 0, 0)?;
                let exclusion_end = self.assembler.label();
                self.assembler.mark(exclusion_end);
                if exclude_from_generator {
                    self.control
                        .generator_region_exclusions
                        .push((exclusion_start, exclusion_end));
                }
                for exclusions in &mut self.control.active_with_region_exclusions {
                    exclusions.push((exclusion_start, exclusion_end));
                }
                if exclude_from_exception {
                    if self
                        .control
                        .exclude_condition_not_taken_from_all_exception_regions
                    {
                        for exclusions in &mut self.control.active_exception_region_exclusions {
                            exclusions.push((exclusion_start, exclusion_end));
                        }
                    } else if let Some(exclusions) =
                        self.control.active_exception_region_exclusions.last_mut()
                    {
                        exclusions.push((exclusion_start, exclusion_end));
                    }
                }
                return Ok(());
            }
            self.emit(NOT_TAKEN, 0, 0)?;
            self.assembler
                .set_last_normalized_exception_owner(self.control.active_normal_finally_bodies > 0);
            return Ok(());
        }
        let comparison_is_boolean = if let Expr::Compare(comparison) = expression
            && comparison.ops.len() == 1
        {
            self.compile_expression(&comparison.left)?;
            self.compile_comparison_rhs(comparison.ops[0], &comparison.comparators[0])?;
            self.assembler
                .set_location(self.source_location(expression.range()));
            let (opcode, argument) = comparison_operator_boolean(comparison.ops[0]);
            self.emit(opcode, argument, -1)?;
            true
        } else {
            self.compile_expression(expression)?;
            false
        };
        self.assembler
            .set_location(self.source_location(expression.range()));
        if !comparison_is_boolean {
            self.emit(TO_BOOL, 0, 0)?;
        }
        self.emit_jump_forward(
            if jump_on {
                POP_JUMP_IF_TRUE
            } else {
                POP_JUMP_IF_FALSE
            },
            label,
            -1,
        )?;
        if self.control.exclude_terminal_if_not_taken
            || self.control.exclude_condition_not_taken_from_exception
        {
            let exclude_from_generator = self.control.exclude_condition_not_taken_from_exception
                && self.control.generator_region_start.is_some();
            let exclude_from_exception = self.control.exclude_condition_not_taken_from_exception
                || (self.control.exclude_terminal_if_not_taken
                    && !self.control.active_with_region_exclusions.is_empty());
            self.control.exclude_terminal_if_not_taken = false;
            let exclusion_start = self.assembler.label();
            self.assembler.mark(exclusion_start);
            self.emit(NOT_TAKEN, 0, 0)?;
            self.assembler
                .set_last_normalized_exception_owner(comparison_is_boolean);
            let exclusion_end = self.assembler.label();
            self.assembler.mark(exclusion_end);
            if exclude_from_generator {
                self.control
                    .generator_region_exclusions
                    .push((exclusion_start, exclusion_end));
            }
            for exclusions in &mut self.control.active_with_region_exclusions {
                exclusions.push((exclusion_start, exclusion_end));
            }
            if exclude_from_exception {
                if self
                    .control
                    .exclude_condition_not_taken_from_all_exception_regions
                {
                    for exclusions in &mut self.control.active_exception_region_exclusions {
                        exclusions.push((exclusion_start, exclusion_end));
                    }
                } else if let Some(exclusions) =
                    self.control.active_exception_region_exclusions.last_mut()
                {
                    exclusions.push((exclusion_start, exclusion_end));
                }
            }
            Ok(())
        } else {
            self.emit(NOT_TAKEN, 0, 0)?;
            self.assembler.set_last_normalized_exception_owner(
                comparison_is_boolean || self.control.active_normal_finally_bodies > 0,
            );
            Ok(())
        }
    }

    pub(super) fn compile_store_target(&mut self, expression: &Expr) -> Result<(), CompileError> {
        let previous = self.assembler.location();
        self.assembler
            .set_location(self.source_location(expression.range()));
        let result = self.compile_store_target_inner(expression);
        self.assembler.set_location(previous);
        result
    }

    fn compile_store_target_inner(&mut self, expression: &Expr) -> Result<(), CompileError> {
        match expression {
            Expr::Name(name) => self.store_name(name.id.as_str()),
            Expr::Attribute(attribute) => {
                self.compile_expression(&attribute.value)?;
                let index = self.name_index(attribute.attr.as_str())?;
                self.assembler
                    .set_location(self.source_location(self.attribute_opcode_range(attribute)));
                self.emit(STORE_ATTR, index, -2)
            }
            Expr::Subscript(subscript) => {
                self.compile_expression(&subscript.value)?;
                if let Expr::Slice(slice) = subscript.slice.as_ref()
                    && two_element_slice_optimization(slice)
                {
                    let slice_location = self.source_location(slice.range);
                    self.compile_optional_slice_bound(slice.lower.as_deref(), slice_location)?;
                    self.compile_optional_slice_bound(slice.upper.as_deref(), slice_location)?;
                    self.assembler
                        .set_location(self.source_location(subscript.range));
                    self.emit(STORE_SLICE, 0, -4)
                } else {
                    self.compile_expression(&subscript.slice)?;
                    self.assembler
                        .set_location(self.source_location(subscript.range));
                    self.emit(STORE_SUBSCR, 0, -3)
                }
            }
            Expr::List(list) => self.compile_unpack_target(&list.elts),
            Expr::Tuple(tuple) => self.compile_unpack_target(&tuple.elts),
            Expr::Starred(starred) => self.compile_store_target(&starred.value),
            _ => Err(unsupported("assignment target")),
        }
    }

    fn compile_unpack_target(&mut self, elements: &[Expr]) -> Result<(), CompileError> {
        let starred = elements.iter().position(Expr::is_starred_expr);
        if let Some(starred) = starred {
            let before = to_u32(starred, "unpack target count")?;
            let after = to_u32(elements.len() - starred - 1, "unpack target count")?;
            self.emit(
                UNPACK_EX,
                before | (after << 8),
                i32::try_from(elements.len()).unwrap() - 1,
            )?;
        } else {
            let count = to_u32(elements.len(), "unpack target count")?;
            self.emit(
                UNPACK_SEQUENCE,
                count,
                i32::try_from(elements.len()).unwrap() - 1,
            )?;
        }
        for element in elements {
            self.compile_store_target(element)?;
        }
        Ok(())
    }

    pub(super) fn compile_delete_target(&mut self, expression: &Expr) -> Result<(), CompileError> {
        self.assembler
            .set_location(self.source_location(expression.range()));
        match expression {
            Expr::Name(name) => self.delete_name(name.id.as_str()),
            Expr::Attribute(attribute) => {
                self.compile_expression(&attribute.value)?;
                let index = self.name_index(attribute.attr.as_str())?;
                self.emit(DELETE_ATTR, index, -1)
            }
            Expr::Subscript(subscript) => {
                self.compile_expression(&subscript.value)?;
                self.compile_expression(&subscript.slice)?;
                self.emit(DELETE_SUBSCR, 0, -2)
            }
            Expr::List(list) => {
                for element in &list.elts {
                    self.compile_delete_target(element)?;
                }
                Ok(())
            }
            Expr::Tuple(tuple) => {
                for element in &tuple.elts {
                    self.compile_delete_target(element)?;
                }
                Ok(())
            }
            _ => Err(unsupported("deletion target")),
        }
    }
}

impl CollectionKind {
    const fn build_opcode(self) -> Opcode {
        match self {
            Self::List => BUILD_LIST,
            Self::Tuple => BUILD_TUPLE,
            Self::Set => BUILD_SET,
        }
    }

    const fn build_opcode_for_unpacking(self) -> Opcode {
        match self {
            Self::List | Self::Tuple => BUILD_LIST,
            Self::Set => BUILD_SET,
        }
    }

    const fn append_opcode(self) -> Opcode {
        match self {
            Self::List | Self::Tuple => LIST_APPEND,
            Self::Set => SET_ADD,
        }
    }

    const fn extend_opcode(self) -> Opcode {
        match self {
            Self::List | Self::Tuple => LIST_EXTEND,
            Self::Set => SET_UPDATE,
        }
    }
}
fn expression_contains_yield(expression: &Expr) -> bool {
    #[derive(Default)]
    struct Collector {
        found: bool,
    }

    impl<'ast> Visitor<'ast> for Collector {
        fn visit_expr(&mut self, expression: &'ast Expr) {
            if matches!(expression, Expr::Lambda(_)) {
                return;
            }
            if matches!(expression, Expr::Yield(_) | Expr::YieldFrom(_)) {
                self.found = true;
                return;
            }
            walk_expr(self, expression);
        }
    }

    let mut collector = Collector::default();
    collector.visit_expr(expression);
    collector.found
}

fn expression_contains_inlined_comprehension(expression: &Expr) -> bool {
    #[derive(Default)]
    struct Collector {
        found: bool,
    }

    impl<'ast> Visitor<'ast> for Collector {
        fn visit_expr(&mut self, expression: &'ast Expr) {
            if self.found || matches!(expression, Expr::Lambda(_) | Expr::Generator(_)) {
                return;
            }
            if matches!(
                expression,
                Expr::ListComp(_) | Expr::SetComp(_) | Expr::DictComp(_)
            ) {
                self.found = true;
            } else {
                walk_expr(self, expression);
            }
        }
    }

    let mut collector = Collector::default();
    collector.visit_expr(expression);
    collector.found
}

pub(super) fn optimized_generator_callable(call: &ruff_python_ast::ExprCall) -> Option<&str> {
    let Expr::Name(name) = call.func.as_ref() else {
        return None;
    };
    if !matches!(name.id.as_str(), "all" | "any" | "tuple")
        || !matches!(call.arguments.args.as_ref(), [Expr::Generator(_)])
        || !call.arguments.keywords.is_empty()
    {
        return None;
    }
    Some(name.id.as_str())
}

fn expression_contains_await(expression: &Expr) -> bool {
    #[derive(Default)]
    struct Collector {
        found: bool,
    }

    impl<'ast> Visitor<'ast> for Collector {
        fn visit_expr(&mut self, expression: &'ast Expr) {
            if matches!(expression, Expr::Lambda(_)) {
                return;
            }
            if matches!(expression, Expr::Await(_)) {
                self.found = true;
                return;
            }
            walk_expr(self, expression);
        }
    }

    let mut collector = Collector::default();
    collector.visit_expr(expression);
    collector.found
}
pub(super) fn expression_defers_async_comprehension_restore(expression: &Expr) -> bool {
    match expression {
        Expr::ListComp(comprehension) => comprehension
            .generators
            .iter()
            .any(|generator| generator.is_async),
        Expr::SetComp(comprehension) => comprehension
            .generators
            .iter()
            .any(|generator| generator.is_async),
        Expr::DictComp(comprehension) => comprehension
            .generators
            .iter()
            .any(|generator| generator.is_async),
        _ => false,
    }
}
pub(super) fn literal_truthiness(expression: &Expr) -> Option<bool> {
    match expression {
        Expr::Name(name) if name.ctx == ExprContext::Load && name.id.as_str() == "__debug__" => {
            Some(true)
        }
        Expr::NoneLiteral(_) => Some(false),
        Expr::BooleanLiteral(value) => Some(value.value),
        Expr::NumberLiteral(number) => match &number.value {
            Number::Int(value) => Some(value.as_u64() != Some(0)),
            Number::Float(value) => Some(*value != 0.0),
            Number::Complex { real, imag } => Some(*real != 0.0 || *imag != 0.0),
        },
        Expr::StringLiteral(value) => Some(!value.value.is_empty()),
        Expr::BytesLiteral(value) => Some(!value.value.is_empty()),
        Expr::FString(_) => match fold_constant(expression)? {
            Constant::String(value) => Some(!value.is_empty()),
            _ => None,
        },
        Expr::Tuple(value) => Some(!value.elts.is_empty()),
        _ => None,
    }
}

pub(super) fn jump_constant_truthiness(expression: &Expr) -> Option<bool> {
    match expression {
        Expr::EllipsisLiteral(_) => Some(true),
        Expr::Name(name) if name.ctx == ExprContext::Load && name.id.as_str() == "__debug__" => {
            Some(true)
        }
        Expr::NoneLiteral(_)
        | Expr::BooleanLiteral(_)
        | Expr::NumberLiteral(_)
        | Expr::StringLiteral(_)
        | Expr::BytesLiteral(_)
        | Expr::FString(_) => literal_truthiness(expression),
        _ => None,
    }
}

pub(super) fn early_condition_truthiness(expression: &Expr) -> Option<bool> {
    if matches!(expression, Expr::Tuple(_)) {
        None
    } else {
        literal_truthiness(expression)
    }
}

fn comprehension_filter_truthiness(expression: &Expr) -> Option<bool> {
    if let Expr::UnaryOp(unary) = expression
        && unary.op == UnaryOp::Not
    {
        jump_constant_truthiness(&unary.operand).map(|truthiness| !truthiness)
    } else {
        early_condition_truthiness(expression)
    }
}
pub(super) fn binary_operator(operator: Operator, inplace: bool) -> BinaryOperation {
    let operation = match operator {
        Operator::Add => BinaryOperation::ADD,
        Operator::BitAnd => BinaryOperation::AND,
        Operator::FloorDiv => BinaryOperation::FLOOR_DIVIDE,
        Operator::LShift => BinaryOperation::LSHIFT,
        Operator::MatMult => BinaryOperation::MATRIX_MULTIPLY,
        Operator::Mult => BinaryOperation::MULTIPLY,
        Operator::Mod => BinaryOperation::REMAINDER,
        Operator::BitOr => BinaryOperation::OR,
        Operator::Pow => BinaryOperation::POWER,
        Operator::RShift => BinaryOperation::RSHIFT,
        Operator::Sub => BinaryOperation::SUBTRACT,
        Operator::Div => BinaryOperation::TRUE_DIVIDE,
        Operator::BitXor => BinaryOperation::XOR,
    };
    if inplace {
        operation.inplace()
    } else {
        operation
    }
}

pub(super) fn comparison_operator(operator: CmpOp) -> (Opcode, u32) {
    match operator {
        CmpOp::Lt => (COMPARE_OP, ComparisonOperation::LT.argument()),
        CmpOp::LtE => (COMPARE_OP, ComparisonOperation::LE.argument()),
        CmpOp::Eq => (COMPARE_OP, ComparisonOperation::EQ.argument()),
        CmpOp::NotEq => (COMPARE_OP, ComparisonOperation::NE.argument()),
        CmpOp::Gt => (COMPARE_OP, ComparisonOperation::GT.argument()),
        CmpOp::GtE => (COMPARE_OP, ComparisonOperation::GE.argument()),
        CmpOp::Is => (IS_OP, 0),
        CmpOp::IsNot => (IS_OP, 1),
        CmpOp::In => (CONTAINS_OP, 0),
        CmpOp::NotIn => (CONTAINS_OP, 1),
    }
}

pub(super) fn comparison_operator_boolean(operator: CmpOp) -> (Opcode, u32) {
    let (opcode, argument) = comparison_operator(operator);
    if matches!(
        operator,
        CmpOp::Lt | CmpOp::LtE | CmpOp::Eq | CmpOp::NotEq | CmpOp::Gt | CmpOp::GtE
    ) {
        (opcode, ComparisonOperation::force_boolean(argument))
    } else {
        (opcode, argument)
    }
}
