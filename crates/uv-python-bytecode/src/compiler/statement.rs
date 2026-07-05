// Compiler submodules intentionally share the parent-owned state and lowering vocabulary.
#![allow(clippy::wildcard_imports)]

use super::*;

impl Compiler {
    fn loop_context(
        &self,
        continue_label: Label,
        break_label: Label,
        iterator_cleanup: IteratorCleanup,
        break_returns: bool,
        preserve_break_exit: bool,
    ) -> LoopContext {
        LoopContext {
            continue_label,
            break_label,
            iterator_cleanup,
            with_depth: self.control.active_with_exits.len(),
            finally_end_depth: self.control.active_finally_end_blocks,
            exception_region_depth: self.control.active_exception_region_exclusions.len(),
            break_returns,
            preserve_break_exit,
        }
    }

    fn with_loop_context<T>(
        &mut self,
        context: LoopContext,
        compile: impl FnOnce(&mut Self) -> Result<T, CompileError>,
    ) -> Result<T, CompileError> {
        let previous_depth = self.control.loops.len();
        self.control.loops.push(context);
        let result = compile(self);
        debug_assert!(
            result.is_err() || self.control.loops.len() == previous_depth + 1,
            "loop contexts should be balanced"
        );
        self.control.loops.truncate(previous_depth);
        result
    }

    fn with_exception_handler<T>(
        &mut self,
        context: ExceptionHandlerContext,
        compile: impl FnOnce(&mut Self) -> Result<T, CompileError>,
    ) -> Result<(T, Vec<(Label, Label)>), CompileError> {
        let ((value, _context), exclusions) = self.with_control_stack(
            |control| &mut control.active_exception_region_exclusions,
            Vec::new(),
            |compiler| {
                compiler.with_control_stack(
                    |control| &mut control.active_exception_handlers,
                    context,
                    compile,
                )
            },
        )?;
        Ok((value, exclusions))
    }

    pub(super) fn with_comprehension_context<T>(
        &mut self,
        cleanup: (Label, u32),
        compile: impl FnOnce(&mut Self) -> Result<T, CompileError>,
    ) -> Result<(T, Vec<(Label, Label)>), CompileError> {
        let ((value, exclusions), _cleanup) = self.with_control_stack(
            |control| &mut control.active_comprehension_cleanups,
            cleanup,
            |compiler| {
                compiler.with_control_stack(
                    |control| &mut control.active_comprehension_region_exclusions,
                    Vec::new(),
                    compile,
                )
            },
        )?;
        Ok((value, exclusions))
    }

    fn with_control_stack<T, R>(
        &mut self,
        select: for<'a> fn(&'a mut ControlFlowState) -> &'a mut Vec<T>,
        value: T,
        compile: impl FnOnce(&mut Self) -> Result<R, CompileError>,
    ) -> Result<(R, T), CompileError> {
        let previous_depth = select(&mut self.control).len();
        select(&mut self.control).push(value);
        let result = compile(self);
        let entries = select(&mut self.control);
        debug_assert!(
            result.is_err() || entries.len() == previous_depth + 1,
            "scoped control-flow stacks should be balanced"
        );
        entries.truncate(previous_depth + 1);
        let value = entries
            .pop()
            .expect("scoped control-flow stack retains its entry");
        result.map(|result| (result, value))
    }

    fn with_control_counter<R>(
        &mut self,
        select: for<'a> fn(&'a mut ControlFlowState) -> &'a mut usize,
        compile: impl FnOnce(&mut Self) -> Result<R, CompileError>,
    ) -> Result<R, CompileError> {
        let previous = *select(&mut self.control);
        *select(&mut self.control) = previous + 1;
        let result = compile(self);
        *select(&mut self.control) = previous;
        result
    }

    fn with_finally_protected_context<T>(
        &mut self,
        pass_location: Option<SourceLocation>,
        overriding_return: bool,
        preserve_break_exit_loop_range: Option<ruff_text_size::TextRange>,
        return_context: Option<ReturnFinallyContext>,
        compile: impl FnOnce(&mut Self) -> Result<T, CompileError>,
    ) -> Result<(T, Vec<(Label, Label)>), CompileError> {
        let exclusion_depth = self.control.active_exception_region_exclusions.len();
        let pass_depth = self.control.active_pass_finally_locations.len();
        let return_depth = self.control.active_return_finally_contexts.len();
        let has_pass_location = pass_location.is_some();
        let has_return_context = return_context.is_some();
        self.control
            .active_exception_region_exclusions
            .push(Vec::new());
        if let Some(location) = pass_location {
            self.control.active_pass_finally_locations.push(location);
        }
        if let Some(context) = return_context {
            self.control.active_return_finally_contexts.push(context);
        }
        let previous_overriding_returns = self.control.active_overriding_finally_returns;
        self.control.active_overriding_finally_returns += usize::from(overriding_return);
        let previous_break_exit_loop_range = std::mem::replace(
            &mut self.control.preserve_finally_break_exit_loop_range,
            preserve_break_exit_loop_range,
        );
        let previous_try_bodies = self.control.active_finally_try_bodies;
        self.control.active_finally_try_bodies = previous_try_bodies + 1;

        let result = compile(self);

        self.control.active_finally_try_bodies = previous_try_bodies;
        self.control.preserve_finally_break_exit_loop_range = previous_break_exit_loop_range;
        self.control.active_overriding_finally_returns = previous_overriding_returns;
        debug_assert!(
            result.is_err()
                || self.control.active_return_finally_contexts.len()
                    == return_depth + usize::from(has_return_context),
            "return-finally contexts should be balanced"
        );
        self.control
            .active_return_finally_contexts
            .truncate(return_depth);
        debug_assert!(
            result.is_err()
                || self.control.active_pass_finally_locations.len()
                    == pass_depth + usize::from(has_pass_location),
            "pass-finally locations should be balanced"
        );
        self.control
            .active_pass_finally_locations
            .truncate(pass_depth);
        debug_assert!(
            result.is_err()
                || self.control.active_exception_region_exclusions.len() == exclusion_depth + 1,
            "finally exception exclusions should be balanced"
        );
        self.control
            .active_exception_region_exclusions
            .truncate(exclusion_depth + 1);
        let exclusions = self
            .control
            .active_exception_region_exclusions
            .pop()
            .expect("finally protected region retains its exclusion collector");
        result.map(|value| (value, exclusions))
    }

    fn with_region_exclusion_collector<T>(
        &mut self,
        select: for<'a> fn(&'a mut ControlFlowState) -> &'a mut RegionExclusionStack,
        compile: impl FnOnce(&mut Self) -> Result<T, CompileError>,
    ) -> Result<(T, RegionExclusions), CompileError> {
        let (value, exclusions) = self.with_control_stack(select, Vec::new(), compile)?;
        Ok((value, exclusions))
    }

    fn with_with_context<T>(
        &mut self,
        context: WithExitContext,
        terminal: bool,
        compile: impl FnOnce(&mut Self) -> Result<T, CompileError>,
    ) -> Result<T, CompileError> {
        let previous_depth = self.control.active_with_exits.len();
        let previous_terminal_withs = self.control.active_terminal_withs;
        self.control.active_with_exits.push(context);
        self.control.active_terminal_withs += usize::from(terminal);
        let result = compile(self);
        self.control.active_terminal_withs = previous_terminal_withs;
        debug_assert!(
            result.is_err() || self.control.active_with_exits.len() == previous_depth + 1,
            "with contexts should be balanced"
        );
        self.control.active_with_exits.truncate(previous_depth);
        result
    }

    pub(super) fn with_control_override<T, R>(
        &mut self,
        select: for<'a> fn(&'a mut ControlFlowState) -> &'a mut T,
        value: T,
        compile: impl FnOnce(&mut Self) -> Result<R, CompileError>,
    ) -> Result<R, CompileError> {
        let previous = std::mem::replace(select(&mut self.control), value);
        let result = compile(self);
        *select(&mut self.control) = previous;
        result
    }

    pub(super) fn with_fallthrough_tracking<T>(
        &mut self,
        compile: impl FnOnce(&mut Self) -> Result<T, CompileError>,
    ) -> Result<(T, bool), CompileError> {
        let previous = std::mem::replace(&mut self.control.emitted_fallthrough_return, false);
        let result = compile(self);
        let emitted = std::mem::replace(&mut self.control.emitted_fallthrough_return, previous);
        result.map(|value| (value, emitted))
    }

    pub(super) fn compile_suite(&mut self, body: &[Stmt]) -> Result<(), CompileError> {
        self.compile_suite_inner(body, false)
    }

    pub(super) fn compile_suite_inner(
        &mut self,
        body: &[Stmt],
        final_expression_becomes_return: bool,
    ) -> Result<(), CompileError> {
        for (index, statement) in body.iter().enumerate() {
            let is_final = index + 1 == body.len();
            if is_final && final_expression_becomes_return {
                match statement {
                    Stmt::While(statement)
                        if statement.orelse.is_empty()
                            && !suite_contains_loop_break(&statement.body)
                            && matches!(statement.test.as_ref(), Expr::BoolOp(boolean) if boolean.op == BoolOp::And && boolean.values.len() > 1) =>
                    {
                        self.compile_terminal_while_and(statement)?;
                        self.control.emitted_fallthrough_return = true;
                        continue;
                    }
                    Stmt::While(statement)
                        if statement.orelse.is_empty()
                            && suite_contains_loop_break(&statement.body) =>
                    {
                        self.compile_terminal_while_break(statement)?;
                        self.control.emitted_fallthrough_return = true;
                        continue;
                    }
                    Stmt::For(statement)
                        if !statement.is_async
                            && statement.orelse.is_empty()
                            && matches!(statement.body.as_slice(), [Stmt::Break(_)]) =>
                    {
                        self.compile_terminal_for_break(statement)?;
                        self.control.emitted_fallthrough_return = true;
                        continue;
                    }
                    Stmt::If(statement) => {
                        self.compile_terminal_if(statement)?;
                        self.control.emitted_fallthrough_return = true;
                        continue;
                    }
                    Stmt::Expr(expression) if matches!(expression.value.as_ref(), Expr::If(_)) => {
                        let Expr::If(expression) = expression.value.as_ref() else {
                            unreachable!();
                        };
                        self.compile_terminal_if_expression(expression)?;
                        self.control.emitted_fallthrough_return = true;
                        continue;
                    }
                    Stmt::Expr(expression) if matches!(expression.value.as_ref(), Expr::Tuple(tuple) if matches!(tuple.elts.last(), Some(Expr::If(_)))) =>
                    {
                        let Expr::Tuple(tuple) = expression.value.as_ref() else {
                            unreachable!();
                        };
                        self.compile_terminal_tuple_if_expression(tuple)?;
                        self.control.emitted_fallthrough_return = true;
                        continue;
                    }
                    Stmt::Assign(assignment)
                        if matches!(assignment.value.as_ref(), Expr::If(_)) =>
                    {
                        let Expr::If(expression) = assignment.value.as_ref() else {
                            unreachable!();
                        };
                        self.compile_terminal_assignment_if_expression(assignment, expression)?;
                        self.control.emitted_fallthrough_return = true;
                        continue;
                    }
                    Stmt::With(statement) if !statement.is_async => {
                        self.compile_with_items(&statement.items, &statement.body, true)?;
                        self.control.emitted_fallthrough_return = true;
                        continue;
                    }
                    Stmt::With(statement) => {
                        self.compile_async_with_items(&statement.items, &statement.body, true)?;
                        self.control.emitted_fallthrough_return = true;
                        continue;
                    }
                    Stmt::Try(statement) => {
                        let previous = self.assembler.location();
                        self.assembler
                            .set_location(self.source_location(statement.range));
                        let result = self.compile_try_inner(statement, true, true, false);
                        self.assembler.set_location(previous);
                        result?;
                        self.control.emitted_fallthrough_return = true;
                        continue;
                    }
                    Stmt::Match(statement) => {
                        self.compile_match_inner(statement, true)?;
                        self.control.emitted_fallthrough_return = true;
                        continue;
                    }
                    Stmt::Expr(expression)
                        if matches!(expression.value.as_ref(), Expr::BoolOp(_)) =>
                    {
                        let Expr::BoolOp(expression) = expression.value.as_ref() else {
                            unreachable!();
                        };
                        self.compile_terminal_bool_expression(expression)?;
                        self.control.emitted_fallthrough_return = true;
                        continue;
                    }
                    Stmt::Expr(expression) if matches!(expression.value.as_ref(), Expr::Compare(compare) if compare.ops.len() > 1) =>
                    {
                        let Expr::Compare(expression) = expression.value.as_ref() else {
                            unreachable!();
                        };
                        self.compile_terminal_compare_expression(expression)?;
                        self.control.emitted_fallthrough_return = true;
                        continue;
                    }
                    _ => {}
                }
            }
            if let Stmt::Pass(statement) = statement
                // CPython retains pass markers in the exceptional copy of a multi-statement
                // finally body, even though ordinary protected suites omit them.
                && (self.control.active_exception_region_exclusions.is_empty()
                    || self.control.active_finally_end_blocks > 0)
                && (self.control.active_with_region_exclusions.is_empty() || body.len() > 1)
            {
                self.assembler
                    .set_location(self.source_location(statement.range));
                let noop_start = self.assembler.label();
                self.assembler.mark(noop_start);
                self.emit(NOP, 0, 0)?;
                let noop_end = self.assembler.label();
                self.assembler.mark(noop_end);
                if is_final {
                    for exclusions in &mut self.control.active_with_region_exclusions {
                        exclusions.push((noop_start, noop_end));
                    }
                }
            } else if let Stmt::Expr(expression) = statement
                && fold_constant(&expression.value).is_some()
            {
                if literal_constant(&expression.value).is_none() {
                    self.record_folded_value(&expression.value)?;
                }
                let range = if let Expr::FString(fstring) = expression.value.as_ref() {
                    self.fstring_result_range(fstring)
                } else {
                    statement.range()
                };
                self.assembler.set_location(self.source_location(range));
                if index + 1 < body.len()
                    || (!final_expression_becomes_return
                        && !matches!(self.unit.scope, Scope::Class { .. }))
                {
                    let noop_start = self.assembler.label();
                    self.assembler.mark(noop_start);
                    self.emit(NOP, 0, 0)?;
                    let noop_end = self.assembler.label();
                    self.assembler.mark(noop_end);
                    let final_statement = index + 1 == body.len();
                    if final_statement && self.control.generator_region_start.is_some() {
                        self.control
                            .generator_region_exclusions
                            .push((noop_start, noop_end));
                    }
                    if final_statement {
                        for exclusions in &mut self.control.active_with_region_exclusions {
                            exclusions.push((noop_start, noop_end));
                        }
                        for exclusions in &mut self.control.active_exception_region_exclusions {
                            exclusions.push((noop_start, noop_end));
                        }
                    }
                }
            } else {
                let emit_protected_async_for_end_nop = is_final
                    && matches!(statement, Stmt::For(statement) if statement.is_async)
                    && (!self.control.active_with_region_exclusions.is_empty()
                        || !self.control.active_exception_region_exclusions.is_empty());
                self.with_control_override(
                    |control| &mut control.emit_protected_async_for_end_nop,
                    emit_protected_async_for_end_nop,
                    |compiler| compiler.compile_statement(statement),
                )?;
            }
            if statement_terminates(statement) {
                let unreachable = &body[index + 1..];
                // CPython visits unreachable statements before the flow graph removes them.
                // Its constant compaction always retains slot zero, so the first literal in
                // an unreachable tail survives when no reachable constant preceded it.
                if self.output.constants.is_empty()
                    && let Some(constant) = first_suite_literal_constant(unreachable)
                {
                    self.add_constant(constant)?;
                }
                self.pre_register_suite_names(unreachable)?;
                break;
            }
        }
        Ok(())
    }

    fn emit_implicit_return(&mut self) -> Result<(), CompileError> {
        let none = self.add_constant(Constant::None)?;
        self.emit(LOAD_CONST, none, 1)?;
        self.emit(RETURN_VALUE, 0, -1)
    }

    pub(super) fn emit_deferred_implicit_return(&mut self) -> Result<(), CompileError> {
        let initialized_locals = self
            .control
            .unwind_exception_handlers_for_implicit_return
            .then(|| self.symbols.initialized_locals.clone());
        let exclusion_start = (self.control.unwind_exception_handlers_for_implicit_return
            && !self.control.active_exception_region_exclusions.is_empty())
        .then(|| {
            let start = self.assembler.label();
            self.assembler.mark(start);
            start
        });
        if self.control.unwind_exception_handlers_for_implicit_return {
            for index in (0..self.control.active_exception_handlers.len()).rev() {
                let name = self.control.active_exception_handlers[index].name.clone();
                self.emit(POP_EXCEPT, 0, -1)?;
                if let Some(name) = &name {
                    self.emit_deferred_constant_before_return(Constant::None)?;
                    self.store_name(name)?;
                    self.delete_name(name)?;
                }
            }
        }
        self.emit_deferred_constant_before_return(Constant::None)?;
        self.emit(RETURN_VALUE, 0, -1)?;
        if let Some(exclusion_start) = exclusion_start {
            let exclusion_end = self.assembler.label();
            self.assembler.mark(exclusion_end);
            for exclusions in &mut self.control.active_exception_region_exclusions {
                exclusions.push((exclusion_start, exclusion_end));
            }
        }
        if let Some(initialized_locals) = initialized_locals {
            // The cleanup edge returned, so its deletes do not affect subsequently emitted cold
            // handler blocks.
            self.symbols.initialized_locals = initialized_locals;
        }
        Ok(())
    }

    fn compile_terminal_while_break(
        &mut self,
        statement: &ruff_python_ast::StmtWhile,
    ) -> Result<(), CompileError> {
        let base_depth = self.control.depth;
        let start = self.assembler.label();
        let condition_false = self.assembler.label();
        self.assembler.mark(start);
        if early_condition_truthiness(&statement.test) == Some(true) {
            if self.output.constants.is_empty()
                && let Some(constant) = fold_constant(&statement.test)
            {
                self.add_constant(constant)?;
            }
            self.assembler
                .set_location(self.source_location(statement.test.range()));
            self.emit(NOP, 0, 0)?;
        } else {
            self.compile_jump_if(&statement.test, false, condition_false)?;
        }
        if self.output.constants.is_empty()
            && let Some(constant) = first_suite_literal_constant(&statement.body)
        {
            self.add_constant(constant)?;
        }
        let context = self.loop_context(start, condition_false, IteratorCleanup::None, true, false);
        self.with_loop_context(context, |compiler| compiler.compile_suite(&statement.body))?;
        if !suite_terminates(&statement.body) {
            if let Some(location) = self.assembler.last_instruction_location() {
                self.assembler.set_location(location);
            }
            self.emit_jump_backward(JUMP_BACKWARD, start, 0)?;
        }

        self.assembler.mark(condition_false);
        self.set_depth(base_depth);
        self.assembler
            .set_location(self.source_location(statement.test.range()));
        self.emit_implicit_return()?;
        self.set_depth(base_depth);
        Ok(())
    }

    fn compile_terminal_while_and(
        &mut self,
        statement: &ruff_python_ast::StmtWhile,
    ) -> Result<(), CompileError> {
        let Expr::BoolOp(condition) = statement.test.as_ref() else {
            unreachable!("terminal while-and requires a boolean condition");
        };
        let base_depth = self.control.depth;
        let start = self.assembler.label();
        self.assembler.mark(start);
        let exits = condition
            .values
            .iter()
            .map(|value| {
                let exit = self.assembler.label();
                self.compile_jump_if(value, false, exit)?;
                Ok((value, exit))
            })
            .collect::<Result<Vec<_>, CompileError>>()?;

        let context = self.loop_context(start, exits[0].1, IteratorCleanup::None, true, false);
        let body_start = self.assembler.instruction_count();
        self.with_loop_context(context, |compiler| compiler.compile_suite(&statement.body))?;
        if !suite_terminates(&statement.body) {
            self.emit_while_backedge(&statement.body, body_start, start)?;
        }

        for (value, exit) in exits.into_iter().rev() {
            self.assembler.mark(exit);
            self.set_depth(base_depth);
            let range = match value {
                Expr::UnaryOp(unary) if unary.op == UnaryOp::Not => unary.operand.range(),
                _ => value.range(),
            };
            self.assembler.set_location(self.source_location(range));
            self.emit_implicit_return()?;
        }
        self.set_depth(base_depth);
        Ok(())
    }

    fn compile_terminal_for_break(
        &mut self,
        statement: &ruff_python_ast::StmtFor,
    ) -> Result<(), CompileError> {
        let base_depth = self.control.depth;
        let cleanup = self.assembler.label();
        self.compile_expression(&statement.iter)?;
        self.assembler
            .set_location(self.source_location(statement.iter.range()));
        self.emit(GET_ITER, 0, 0)?;
        self.emit_jump_forward(FOR_ITER, cleanup, 1)?;
        self.compile_store_target(&statement.target)?;
        self.assembler
            .set_location(self.source_location(statement.body[0].range()));
        self.emit(POP_TOP, 0, -1)?;
        self.emit_implicit_return()?;

        self.assembler.mark(cleanup);
        self.set_depth(base_depth + 2);
        self.assembler
            .set_location(self.source_location(statement.iter.range()));
        self.emit(END_FOR, 0, -1)?;
        self.emit(POP_ITER, 0, -1)?;
        self.emit_implicit_return()?;
        self.set_depth(base_depth);
        Ok(())
    }

    fn compile_terminal_if(
        &mut self,
        statement: &ruff_python_ast::StmtIf,
    ) -> Result<(), CompileError> {
        if statement.elif_else_clauses.is_empty()
            && let Expr::Compare(comparison) = statement.test.as_ref()
            && comparison.ops.len() > 1
        {
            return self.compile_terminal_if_chained_compare(statement, comparison);
        }
        if let Some(truthiness) = early_condition_truthiness(&statement.test)
            && statement.elif_else_clauses.len() <= 1
            && statement
                .elif_else_clauses
                .first()
                .is_none_or(|clause| clause.test.is_none())
        {
            if let Some(constant) = fold_constant(&statement.test)
                && (self.output.constants.is_empty() || matches!(constant, Constant::None))
            {
                self.add_constant(constant)?;
            }
            if !truthiness && statement.elif_else_clauses.is_empty() {
                self.pre_register_suite_names(&statement.body)?;
                self.assembler
                    .set_location(self.source_location(statement.test.range()));
                self.emit_deferred_implicit_return()?;
                return Ok(());
            }
            self.assembler
                .set_location(self.source_location(statement.test.range()));
            self.emit(NOP, 0, 0)?;
            let body = if truthiness {
                &statement.body
            } else if let Some(clause) = statement.elif_else_clauses.first() {
                self.pre_register_suite_names(&statement.body)?;
                &clause.body
            } else {
                self.pre_register_suite_names(&statement.body)?;
                self.emit_deferred_implicit_return()?;
                return Ok(());
            };
            let emitted_fallthrough = if matches!(body.last(), Some(Stmt::If(_))) {
                let ((), emitted_fallthrough) = self.with_fallthrough_tracking(|compiler| {
                    compiler.compile_suite_inner(body, true)
                })?;
                emitted_fallthrough
            } else {
                self.compile_suite(body)?;
                false
            };
            if truthiness {
                for clause in &statement.elif_else_clauses {
                    self.pre_register_suite_names(&clause.body)?;
                }
            }
            if !emitted_fallthrough {
                if let Some(location) = self.assembler.last_instruction_location() {
                    self.assembler.set_location(location);
                }
                self.emit_deferred_implicit_return()?;
            }
            return Ok(());
        }

        let base_depth = self.control.depth;
        let mut branches: Vec<(Option<&Expr>, &[Stmt])> =
            vec![(Some(&statement.test), statement.body.as_slice())];
        branches.extend(
            statement
                .elif_else_clauses
                .iter()
                .map(|clause| (clause.test.as_ref(), clause.body.as_slice())),
        );
        let has_else = branches.last().is_some_and(|(test, _)| test.is_none());

        for (branch_index, (test, body)) in branches.into_iter().enumerate() {
            if let Some(test) = test {
                let next = self.assembler.label();
                let exclude_not_taken = self.control.exclude_terminal_if_not_taken
                    || (!self.control.active_with_region_exclusions.is_empty()
                        && branch_index > 0
                        && suite_terminates(body));
                let exclude_from_generator = self.unit.flags & CO_COROUTINE != 0
                    && self.control.generator_region_start.is_some()
                    && self.assembler.contains_opcode(YIELD_VALUE)
                    && self.control.active_with_region_exclusions.is_empty()
                    && self.control.active_exception_region_exclusions.is_empty()
                    && self.control.active_normal_finally_bodies == 0;
                self.with_control_override(
                    |control| &mut control.exclude_terminal_if_not_taken,
                    exclude_not_taken,
                    |compiler| {
                        compiler.with_control_override(
                            |control| &mut control.exclude_condition_not_taken_from_exception,
                            exclude_from_generator,
                            |compiler| compiler.compile_jump_if(test, false, next),
                        )
                    },
                )?;
                self.mark_definitely_evaluated_locals(test);
                let body_start = self.assembler.instruction_count();
                let ((), emitted_fallthrough) = self.with_fallthrough_tracking(|compiler| {
                    compiler.compile_suite_inner(body, true)
                })?;
                if !suite_terminates(body) && !emitted_fallthrough {
                    if let Some(statement) = body
                        .last()
                        .filter(|statement| statement_uses_implicit_return_location(statement))
                    {
                        self.assembler.set_location(
                            self.source_location(implicit_return_range(self, statement)),
                        );
                    } else if self.assembler.instruction_count() > body_start
                        && let Some(location) = self.assembler.last_instruction_location()
                    {
                        self.assembler.set_location(location);
                    } else if let Some(statement) = body.last() {
                        self.assembler
                            .set_location(self.source_location(statement.range()));
                    } else {
                        self.assembler
                            .set_location(self.source_location(test.range()));
                    }
                    self.emit_deferred_implicit_return()?;
                }
                self.assembler.mark(next);
                self.set_depth(base_depth);
                self.assembler
                    .set_location(self.source_location(test.range()));
            } else {
                let body_start = self.assembler.instruction_count();
                let ((), emitted_fallthrough) = self.with_fallthrough_tracking(|compiler| {
                    compiler.compile_suite_inner(body, true)
                })?;
                if !suite_terminates(body) && !emitted_fallthrough {
                    if let Some(statement) = body
                        .last()
                        .filter(|statement| statement_uses_implicit_return_location(statement))
                    {
                        self.assembler.set_location(
                            self.source_location(implicit_return_range(self, statement)),
                        );
                    } else if self.assembler.instruction_count() > body_start
                        && let Some(location) = self.assembler.last_instruction_location()
                    {
                        self.assembler.set_location(location);
                    } else if let Some(statement) = body.last() {
                        self.assembler
                            .set_location(self.source_location(statement.range()));
                    }
                    self.emit_deferred_implicit_return()?;
                }
            }
        }
        if !has_else {
            self.assembler.set_location(SourceLocation::NONE);
            self.emit_deferred_implicit_return()?;
        }
        self.set_depth(base_depth);
        Ok(())
    }

    fn compile_terminal_if_chained_compare(
        &mut self,
        statement: &ruff_python_ast::StmtIf,
        comparison: &ruff_python_ast::ExprCompare,
    ) -> Result<(), CompileError> {
        let base_depth = self.control.depth;
        let cleanup = self.assembler.label();
        let body = self.assembler.label();
        let condition_false = self.assembler.label();
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
        self.emit_jump_forward(POP_JUMP_IF_FALSE, condition_false, -1)?;
        self.emit(NOT_TAKEN, 0, 0)?;
        self.emit_jump_forward(JUMP_FORWARD, body, 0)?;
        self.assembler.prevent_last_jump_inlining();

        self.assembler.mark(cleanup);
        self.set_depth(base_depth + 1);
        self.assembler
            .set_location(self.source_location(comparison.range));
        self.emit(POP_TOP, 0, -1)?;
        self.emit_deferred_implicit_return()?;

        self.assembler.mark(body);
        self.set_depth(base_depth);
        self.compile_suite(&statement.body)?;
        if let Some(last) = statement.body.last() {
            self.assembler
                .set_location(self.source_location(last.range()));
        }
        self.emit_deferred_implicit_return()?;

        self.assembler.mark(condition_false);
        self.set_depth(base_depth);
        self.assembler
            .set_location(self.source_location(comparison.range));
        self.emit_deferred_implicit_return()?;
        self.set_depth(base_depth);
        Ok(())
    }

    fn compile_terminal_if_expression(
        &mut self,
        expression: &ruff_python_ast::ExprIf,
    ) -> Result<(), CompileError> {
        let base_depth = self.control.depth;
        let otherwise = self.assembler.label();
        self.compile_jump_if(&expression.test, false, otherwise)?;
        self.compile_expression(&expression.body)?;
        self.assembler
            .set_location(self.source_location(expression.body.range()));
        self.emit(POP_TOP, 0, -1)?;
        self.emit_implicit_return()?;

        self.assembler.mark(otherwise);
        self.set_depth(base_depth);
        self.compile_expression(&expression.orelse)?;
        // CPython's conditional-expression join is a jump target. Its redundant-pair
        // optimizer therefore does not remove a constant else value with this POP_TOP.
        let end = self.assembler.label();
        self.assembler.mark(end);
        self.assembler
            .set_location(self.source_location(expression.orelse.range()));
        self.emit(POP_TOP, 0, -1)?;
        self.emit_implicit_return()?;
        self.set_depth(base_depth);
        Ok(())
    }

    fn compile_terminal_tuple_if_expression(
        &mut self,
        tuple: &ruff_python_ast::ExprTuple,
    ) -> Result<(), CompileError> {
        let Some((Expr::If(expression), leading)) = tuple.elts.split_last() else {
            return Err(CompileError::Internal(
                "terminal tuple does not end in a conditional expression".to_string(),
            ));
        };
        let base_depth = self.control.depth;
        for element in leading {
            self.compile_expression(element)?;
        }
        let branch_depth = self.control.depth;
        let otherwise = self.assembler.label();
        self.compile_jump_if(&expression.test, false, otherwise)?;
        self.compile_expression(&expression.body)?;
        self.assembler
            .set_location(self.source_location(tuple.range));
        self.emit_build(BUILD_TUPLE, tuple.elts.len())?;
        self.emit(POP_TOP, 0, -1)?;
        self.emit_implicit_return()?;

        self.assembler.mark(otherwise);
        self.set_depth(branch_depth);
        self.compile_expression(&expression.orelse)?;
        self.assembler
            .set_location(self.source_location(tuple.range));
        self.emit_build(BUILD_TUPLE, tuple.elts.len())?;
        self.emit(POP_TOP, 0, -1)?;
        self.emit_implicit_return()?;
        self.set_depth(base_depth);
        Ok(())
    }

    fn compile_terminal_assignment_if_expression(
        &mut self,
        assignment: &ruff_python_ast::StmtAssign,
        expression: &ruff_python_ast::ExprIf,
    ) -> Result<(), CompileError> {
        self.compile_terminal_assignment_if_expression_inner(assignment, expression, false)
    }

    fn compile_terminal_assignment_if_expression_inner(
        &mut self,
        assignment: &ruff_python_ast::StmtAssign,
        expression: &ruff_python_ast::ExprIf,
        preserve_fallthrough_join_nop: bool,
    ) -> Result<(), CompileError> {
        let base_depth = self.control.depth;
        let otherwise = self.assembler.label();
        self.compile_jump_if(&expression.test, false, otherwise)?;
        self.pre_register_expression_names(&expression.body)?;
        self.pre_register_expression_names(&expression.orelse)?;
        if let Expr::If(body) = expression.body.as_ref() {
            // CPython copies the terminal assignment and return into the nested branches. The
            // nested expression's fallthrough join survives that copy as a source-position NOP.
            self.compile_terminal_assignment_if_expression_inner(assignment, body, true)?;
        } else {
            self.compile_expression(&expression.body)?;
            self.compile_assignment_targets(&assignment.targets)?;
            self.assembler
                .set_location(self.source_location(assignment.targets[0].range()));
            self.emit_deferred_implicit_return()?;
        }

        self.assembler.mark(otherwise);
        self.set_depth(base_depth);
        self.compile_expression(&expression.orelse)?;
        if preserve_fallthrough_join_nop {
            self.assembler
                .set_location(self.source_location(expression.orelse.range()));
            self.emit(NOP, 0, 0)?;
        }
        self.compile_assignment_targets(&assignment.targets)?;
        self.assembler
            .set_location(self.source_location(assignment.targets[0].range()));
        self.emit_deferred_implicit_return()?;
        self.set_depth(base_depth);
        Ok(())
    }

    fn compile_assignment_targets(&mut self, targets: &[Expr]) -> Result<(), CompileError> {
        for (index, target) in targets.iter().enumerate() {
            if index + 1 < targets.len() {
                self.emit(COPY, 1, 1)?;
            }
            self.compile_store_target(target)?;
        }
        Ok(())
    }

    fn compile_terminal_bool_expression(
        &mut self,
        expression: &ruff_python_ast::ExprBoolOp,
    ) -> Result<(), CompileError> {
        let Some((last, leading)) = expression.values.split_last() else {
            return Err(CompileError::Internal(
                "boolean expression contains no values".to_string(),
            ));
        };
        self.assembler
            .set_location(self.source_location(expression.range));
        let base_depth = self.control.depth;
        let jump = match expression.op {
            BoolOp::And => POP_JUMP_IF_FALSE,
            BoolOp::Or => POP_JUMP_IF_TRUE,
        };

        let mut short_circuits = Vec::with_capacity(leading.len());
        for value in leading {
            let short_circuit = self.assembler.label();
            let continue_outer = self.assembler.label();
            short_circuits.push(short_circuit);
            if let Expr::BoolOp(inner) = value {
                self.compile_bool_expression_short_to(
                    inner,
                    if inner.op == expression.op {
                        short_circuit
                    } else {
                        continue_outer
                    },
                )?;
            } else {
                self.compile_expression(value)?;
            }
            self.assembler
                .set_location(self.source_location(expression.range));
            let exclusion_start = self.assembler.label();
            self.assembler.mark(exclusion_start);
            self.emit(COPY, 1, 1)?;
            self.emit(TO_BOOL, 0, 0)?;
            let exclusion_end = self.assembler.label();
            self.assembler.mark(exclusion_end);
            self.emit_jump_forward(jump, short_circuit, -1)?;
            self.emit(NOT_TAKEN, 0, 0)?;
            self.assembler.mark(continue_outer);
            self.emit(POP_TOP, 0, -1)?;
        }

        if let Expr::BoolOp(last) = last {
            self.compile_terminal_bool_expression(last)?;
        } else {
            self.compile_expression(last)?;
            let result = self.assembler.label();
            self.assembler.preserve_block_boundary(result);
            self.assembler.mark(result);
            self.assembler
                .set_location(self.source_location(last.range()));
            self.emit(POP_TOP, 0, -1)?;
            self.emit_implicit_return()?;
        }

        for short_circuit in short_circuits.into_iter().rev() {
            self.assembler.mark(short_circuit);
            self.set_depth(base_depth + 1);
            self.assembler
                .set_location(self.source_location(expression.range));
            self.emit(POP_TOP, 0, -1)?;
            self.emit_implicit_return()?;
        }
        self.set_depth(base_depth);
        Ok(())
    }

    fn compile_bool_expression_short_to(
        &mut self,
        expression: &ruff_python_ast::ExprBoolOp,
        short_circuit: Label,
    ) -> Result<(), CompileError> {
        let Some((last, leading)) = expression.values.split_last() else {
            return Err(CompileError::Internal(
                "boolean expression contains no values".to_string(),
            ));
        };
        self.assembler
            .set_location(self.source_location(expression.range));
        let jump = match expression.op {
            BoolOp::And => POP_JUMP_IF_FALSE,
            BoolOp::Or => POP_JUMP_IF_TRUE,
        };
        for value in leading {
            self.compile_expression(value)?;
            self.emit(COPY, 1, 1)?;
            self.emit(TO_BOOL, 0, 0)?;
            self.emit_jump_forward(jump, short_circuit, -1)?;
            self.emit(NOT_TAKEN, 0, 0)?;
            self.emit(POP_TOP, 0, -1)?;
        }
        self.compile_expression(last)
    }

    fn compile_terminal_compare_expression(
        &mut self,
        expression: &ruff_python_ast::ExprCompare,
    ) -> Result<(), CompileError> {
        let base_depth = self.control.depth;
        let cleanup = self.assembler.label();
        self.assembler
            .set_location(self.source_location(expression.range));
        self.compile_expression(&expression.left)?;
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
        self.assembler
            .set_location(self.source_location(expression.range));
        self.emit(POP_TOP, 0, -1)?;
        self.emit_implicit_return()?;

        self.assembler.mark(cleanup);
        self.set_depth(base_depth + 2);
        self.emit(SWAP, 2, 0)?;
        self.emit(POP_TOP, 0, -1)?;
        self.emit(POP_TOP, 0, -1)?;
        self.emit_implicit_return()?;
        self.set_depth(base_depth);
        Ok(())
    }

    fn compile_statement(&mut self, statement: &Stmt) -> Result<(), CompileError> {
        let previous = self.assembler.location();
        self.assembler
            .set_location(self.source_location(statement.range()));
        let result = self.compile_statement_inner(statement);
        self.assembler.set_location(previous);
        result
    }

    fn compile_statement_inner(&mut self, statement: &Stmt) -> Result<(), CompileError> {
        let starting_depth = self.control.depth;
        match statement {
            Stmt::Assign(assignment) => {
                let unpacked = if let [target] = assignment.targets.as_slice()
                    && let Expr::Tuple(value) = assignment.value.as_ref()
                {
                    let targets = match target {
                        Expr::Tuple(target) => Some(target.elts.as_slice()),
                        Expr::List(target) => Some(target.elts.as_slice()),
                        _ => None,
                    };
                    targets.filter(|targets| {
                        targets.len() == value.elts.len()
                            && targets.len() <= 3
                            && !targets.iter().any(Expr::is_starred_expr)
                            && !value.elts.iter().any(Expr::is_starred_expr)
                    })
                } else {
                    None
                };
                if let Some(targets) = unpacked {
                    let Expr::Tuple(value) = assignment.value.as_ref() else {
                        unreachable!();
                    };
                    for value in &value.elts {
                        self.compile_expression(value)?;
                    }
                    if targets.len() > 1 {
                        self.assembler
                            .set_location(self.source_location(assignment.targets[0].range()));
                        self.emit(SWAP, to_u32(targets.len(), "unpack target count")?, 0)?;
                    }
                    for target in targets {
                        self.compile_store_target(target)?;
                    }
                } else {
                    let defer_restore =
                        expression_defers_async_comprehension_restore(&assignment.value);
                    self.with_control_override(
                        |control| &mut control.defer_async_comprehension_restore,
                        defer_restore,
                        |compiler| {
                            let newly_owned = if assignment
                                .targets
                                .iter()
                                .any(|target| matches!(target, Expr::Name(_)))
                                && let Expr::Name(name) = assignment.value.as_ref()
                            {
                                let name = compiler.required_name_id(name.id.as_str());
                                compiler.symbols.owned_load_locals.insert(name)
                            } else {
                                false
                            };
                            compiler.compile_expression(&assignment.value)?;
                            if newly_owned && let Expr::Name(name) = assignment.value.as_ref() {
                                let name = compiler.required_name_id(name.id.as_str());
                                compiler.symbols.owned_load_locals.remove(name);
                            }
                            Ok(())
                        },
                    )?;
                    for (index, target) in assignment.targets.iter().enumerate() {
                        if index + 1 < assignment.targets.len() {
                            self.emit(COPY, 1, 1)?;
                        }
                        self.compile_store_target(target)?;
                    }
                    if defer_restore {
                        for target in &assignment.targets {
                            let mut names = Vec::new();
                            collect_target_names(target, &mut names);
                            let names = names
                                .into_iter()
                                .map(|name| self.required_name_id(&name))
                                .collect::<Vec<_>>();
                            self.symbols.owned_load_locals.extend(names);
                        }
                    }
                    self.emit_pending_comprehension_restores()?;
                }
            }
            Stmt::AugAssign(assignment) => {
                self.compile_augmented_assignment(assignment)?;
            }
            Stmt::AnnAssign(assignment) => self.compile_annotated_assignment(assignment)?,
            Stmt::Expr(expression) => {
                let discarded_comprehension = match expression.value.as_ref() {
                    Expr::ListComp(comprehension) => {
                        self.compile_comprehension(
                            &comprehension.generators,
                            None,
                            &comprehension.elt,
                            BUILD_LIST,
                            LIST_APPEND,
                            true,
                        )?;
                        true
                    }
                    Expr::SetComp(comprehension) => {
                        self.compile_comprehension(
                            &comprehension.generators,
                            None,
                            &comprehension.elt,
                            BUILD_SET,
                            SET_ADD,
                            true,
                        )?;
                        true
                    }
                    Expr::DictComp(comprehension) => {
                        self.compile_comprehension(
                            &comprehension.generators,
                            comprehension.key.as_deref(),
                            &comprehension.value,
                            BUILD_MAP,
                            MAP_ADD,
                            true,
                        )?;
                        true
                    }
                    _ => {
                        self.compile_expression(&expression.value)?;
                        false
                    }
                };
                if discarded_comprehension {
                    return Ok(());
                }
                self.mark_definitely_evaluated_locals(&expression.value);
                let preserve_no_location = matches!(
                    expression.value.as_ref(),
                    Expr::BoolOp(_) | Expr::If(_)
                ) || matches!(expression.value.as_ref(), Expr::Compare(compare) if compare.ops.len() > 1);
                let optimized_generator_call = matches!(
                    expression.value.as_ref(),
                    Expr::Call(call) if optimized_generator_callable(call).is_some()
                );
                if preserve_no_location || optimized_generator_call {
                    self.assembler.set_location(SourceLocation::NONE);
                } else {
                    let discard_range = match expression.value.as_ref() {
                        Expr::Named(named) => named.target.range(),
                        Expr::Attribute(attribute) => self.attribute_opcode_range(attribute),
                        Expr::Call(call) => self.call_opcode_range(call),
                        Expr::FString(fstring) => self.fstring_result_range(fstring),
                        expression => expression.range(),
                    };
                    if matches!(expression.value.as_ref(), Expr::BinOp(binary) if optimized_percent_format(binary).is_some())
                        && let Some(location) = self.assembler.last_instruction_location()
                    {
                        self.assembler.set_location(location);
                    } else {
                        self.assembler
                            .set_location(self.source_location(discard_range));
                    }
                }
                self.emit(POP_TOP, 0, -1)?;
                if preserve_no_location {
                    self.assembler.preserve_last_no_location();
                }
            }
            Stmt::Pass(_) => {}
            Stmt::If(statement) => self.compile_if(statement)?,
            Stmt::While(statement) => self.compile_while(statement)?,
            Stmt::For(statement) if statement.is_async => self.compile_async_for(statement)?,
            Stmt::For(statement) => self.compile_for(statement)?,
            Stmt::Break(_) => {
                // Loop-control cleanup runs after CPython unwinds the active exception block.
                let unreachable_depth = self.control.depth;
                let context = self
                    .control
                    .loops
                    .last()
                    .copied()
                    .ok_or_else(|| CompileError::Internal("break outside loop".to_string()))?;
                self.emit(NOP, 0, 0)?;
                let finally_end_unwind_start =
                    self.emit_finally_end_unwind(context.finally_end_depth, false)?;
                let exclusion_start = if self.control.active_exception_region_exclusions.len()
                    == context.exception_region_depth
                {
                    None
                } else if finally_end_unwind_start.is_some() {
                    finally_end_unwind_start
                } else {
                    let start = self.assembler.label();
                    self.assembler.mark(start);
                    Some(start)
                };
                self.emit_loop_control_exception_unwind(self.control.loops.len())?;
                let with_unwind_starts = self.emit_active_with_unwind(context.with_depth, false)?;
                match context.iterator_cleanup {
                    IteratorCleanup::None => {}
                    IteratorCleanup::Sync => self.emit(POP_TOP, 0, -1)?,
                    IteratorCleanup::Async => self.emit(POP_TOP, 0, -1)?,
                }
                if context.break_returns {
                    self.emit_implicit_return()?;
                } else {
                    let jump_exclusion_start =
                        (self.control.active_exception_region_exclusions.len()
                            == context.exception_region_depth
                            && context.exception_region_depth > 0
                            && context.finally_end_depth == 0
                            && self.control.active_exception_handlers.is_empty())
                        .then(|| {
                            let start = self.assembler.label();
                            self.assembler.mark(start);
                            start
                        });
                    self.emit_jump_forward(JUMP_FORWARD, context.break_label, 0)?;
                    if context.preserve_break_exit {
                        // CPython inlines small exit blocks before removing a jump to the next
                        // block. Keep this terminal protected-loop break through that pass so
                        // the following finally return is copied onto the edge.
                        self.assembler.defer_last_jump_removal();
                        self.assembler.preserve_last_inlined_jump_nop();
                    }
                    if let Some(jump_exclusion_start) = jump_exclusion_start {
                        let jump_exclusion_end = self.assembler.label();
                        self.assembler.mark(jump_exclusion_end);
                        for exclusions in &mut self.control.active_exception_region_exclusions {
                            exclusions.push((jump_exclusion_start, jump_exclusion_end));
                        }
                    }
                }
                if exclusion_start.is_some() || !with_unwind_starts.is_empty() {
                    let exclusion_end = self.assembler.label();
                    self.assembler.mark(exclusion_end);
                    if let Some(exclusion_start) = exclusion_start {
                        for exclusions in self
                            .control
                            .active_exception_region_exclusions
                            .iter_mut()
                            .skip(context.exception_region_depth)
                        {
                            exclusions.push((exclusion_start, exclusion_end));
                        }
                    }
                    for (index, unwind_start) in with_unwind_starts {
                        self.control.active_with_region_exclusions[index]
                            .push((unwind_start, exclusion_end));
                    }
                }
                self.set_depth(unreachable_depth);
            }
            Stmt::Continue(_) => {
                // Loop-control cleanup runs after CPython unwinds the active exception block.
                let context =
                    self.control.loops.last().copied().ok_or_else(|| {
                        CompileError::Internal("continue outside loop".to_string())
                    })?;
                self.emit(NOP, 0, 0)?;
                let finally_end_unwind_start =
                    self.emit_finally_end_unwind(context.finally_end_depth, false)?;
                let exclusion_start = if self.control.active_exception_region_exclusions.len()
                    == context.exception_region_depth
                {
                    None
                } else if finally_end_unwind_start.is_some() {
                    finally_end_unwind_start
                } else {
                    let start = self.assembler.label();
                    self.assembler.mark(start);
                    Some(start)
                };
                self.emit_loop_control_exception_unwind(self.control.loops.len())?;
                let with_unwind_starts = self.emit_active_with_unwind(context.with_depth, false)?;
                self.emit_jump_backward(JUMP_BACKWARD, context.continue_label, 0)?;
                // CPython emits a temporary NOP before loop control, so an incoming jump sees
                // the NOP instead of threading through this backward jump.
                self.assembler.prevent_last_jump_threading_target();
                if exclusion_start.is_some() || !with_unwind_starts.is_empty() {
                    let exclusion_end = self.assembler.label();
                    self.assembler.mark(exclusion_end);
                    if let Some(exclusion_start) = exclusion_start {
                        for exclusions in self
                            .control
                            .active_exception_region_exclusions
                            .iter_mut()
                            .skip(context.exception_region_depth)
                        {
                            exclusions.push((exclusion_start, exclusion_end));
                        }
                    }
                    for (index, unwind_start) in with_unwind_starts {
                        self.control.active_with_region_exclusions[index]
                            .push((unwind_start, exclusion_end));
                    }
                }
                self.set_depth(starting_depth);
            }
            Stmt::Return(statement) => {
                if !matches!(self.unit.scope, Scope::Function { .. }) {
                    return Err(unsupported("return outside a function"));
                }
                let return_is_overridden = self.control.active_overriding_finally_returns > 0;
                let preserve_tos = statement
                    .value
                    .as_deref()
                    .is_some_and(|value| !is_literal_constant(value));
                let unwinds_pass_finally = !self.control.active_pass_finally_locations.is_empty();
                let overriding_unwind_start = if return_is_overridden
                    && !preserve_tos
                    && !self.control.active_exception_region_exclusions.is_empty()
                {
                    let start = self.assembler.label();
                    self.assembler.mark(start);
                    Some(start)
                } else {
                    None
                };
                if !preserve_tos {
                    let range = statement
                        .value
                        .as_deref()
                        .map_or(statement.range, Ranged::range);
                    self.assembler.set_location(self.source_location(range));
                    self.emit(NOP, 0, 0)?;
                } else if let Some(value) = &statement.value {
                    self.compile_expression(value)?;
                }
                let pass_finally_edge_start = unwinds_pass_finally
                    .then(|| {
                        let start = self.assembler.label();
                        self.assembler
                            .mark_before_trailing_converted_pop_block(start)
                            .then_some(start)
                    })
                    .flatten();
                let nested_finally_region_unwind_start = (self.control.active_finally_end_blocks
                    > 0
                    && self.control.active_exception_region_exclusions.len()
                        > self.control.active_finally_end_blocks)
                    .then(|| {
                        let start = self.assembler.label();
                        self.assembler.mark(start);
                        (self.control.active_finally_end_blocks, start)
                    });
                let finally_end_unwind_start = self.emit_finally_end_unwind(0, preserve_tos)?;
                let delay_exception_unwind_start = overriding_unwind_start.is_none()
                    && !self.control.active_exception_region_exclusions.is_empty()
                    && self.control.active_with_exits.is_empty()
                    && !unwinds_pass_finally
                    && preserve_tos
                    && !self.control.active_exception_handlers.is_empty();
                let mut exception_unwind_start = if overriding_unwind_start.is_some() {
                    overriding_unwind_start
                } else if finally_end_unwind_start.is_some() {
                    finally_end_unwind_start
                } else if !self.control.active_exception_region_exclusions.is_empty()
                    && self.control.active_with_exits.is_empty()
                    && !unwinds_pass_finally
                    && !delay_exception_unwind_start
                {
                    let start = self.assembler.label();
                    self.assembler.mark(start);
                    Some(start)
                } else {
                    None
                };
                let handler_count = if !return_is_overridden
                    && self.control.active_with_exits.is_empty()
                    && !unwinds_pass_finally
                {
                    self.control.active_exception_handlers.len()
                } else {
                    0
                };
                let mut next_loop = self.control.loops.len();
                for handler_index in (0..handler_count).rev() {
                    let (loop_depth, name) = {
                        let handler = &self.control.active_exception_handlers[handler_index];
                        (handler.loop_depth, handler.name.clone())
                    };
                    self.emit_loop_iterator_cleanup(loop_depth, next_loop, preserve_tos)?;
                    if preserve_tos {
                        self.emit(SWAP, 2, 0)?;
                    }
                    if delay_exception_unwind_start && exception_unwind_start.is_none() {
                        let start = self.assembler.label();
                        self.assembler.mark(start);
                        exception_unwind_start = Some(start);
                    }
                    let protected_pop_except = (!return_is_overridden
                        && !self.control.active_return_finally_contexts.is_empty())
                    .then(|| {
                        let start = self.assembler.label();
                        self.assembler.mark(start);
                        start
                    });
                    self.emit(POP_EXCEPT, 0, -1)?;
                    if let Some(start) = protected_pop_except {
                        let end = self.assembler.label();
                        self.assembler.mark(end);
                        for context in &self.control.active_return_finally_contexts {
                            self.assembler.add_exception_region(
                                start,
                                end,
                                context.handler,
                                context.depth,
                                false,
                            );
                        }
                    }
                    if let Some(name) = &name {
                        let none = self.add_constant(Constant::None)?;
                        self.emit(LOAD_CONST, none, 1)?;
                        self.store_name(name)?;
                        self.delete_name(name)?;
                    }
                    next_loop = loop_depth;
                }
                let return_finally_contexts = if !return_is_overridden {
                    std::mem::take(&mut self.control.active_return_finally_contexts)
                } else {
                    Vec::new()
                };
                for finally_context in return_finally_contexts.iter().rev() {
                    self.emit_loop_iterator_cleanup(
                        finally_context.loop_depth,
                        next_loop,
                        preserve_tos,
                    )?;
                    let initialized_locals = self.symbols.initialized_locals.clone();
                    let result = self.compile_suite(&finally_context.body);
                    self.symbols.initialized_locals = initialized_locals;
                    result?;
                    if let Some(location) = self.assembler.last_instruction_location() {
                        self.assembler.set_location(location);
                    }
                    next_loop = finally_context.loop_depth;
                }
                if !return_finally_contexts.is_empty() {
                    self.control.active_return_finally_contexts = return_finally_contexts;
                }
                self.emit_loop_iterator_cleanup(0, next_loop, preserve_tos)?;
                let with_unwind_starts = self.emit_active_with_unwind(0, preserve_tos)?;
                let finally_unwind_start = if unwinds_pass_finally {
                    let start = if let Some(start) = pass_finally_edge_start {
                        start
                    } else {
                        let start = self.assembler.label();
                        self.assembler.mark(start);
                        start
                    };
                    self.emit_pass_finally_nops()?;
                    self.assembler.set_location(SourceLocation::NONE);
                    Some(start)
                } else {
                    None
                };
                if !preserve_tos && !return_is_overridden {
                    if let Some(value) = &statement.value {
                        let return_spans_lines = {
                            let location = self.source_location(statement.range);
                            location.line != location.end_line
                        };
                        if !with_unwind_starts.is_empty() || finally_unwind_start.is_some() {
                            let constant = fold_constant(value).ok_or_else(|| {
                                CompileError::Internal(
                                    "literal return value did not fold to a constant".to_string(),
                                )
                            })?;
                            self.emit_preprocessed_constant(value, constant)?;
                        } else if return_spans_lines {
                            self.assembler
                                .set_location(self.source_location(statement.range));
                            let constant = fold_constant(value).ok_or_else(|| {
                                CompileError::Internal(
                                    "literal return value did not fold to a constant".to_string(),
                                )
                            })?;
                            self.emit_preprocessed_constant(value, constant)?;
                        } else {
                            self.assembler
                                .set_location(self.source_location(value.range()));
                            self.compile_expression(value)?;
                        }
                    } else {
                        let none = self.add_constant(Constant::None)?;
                        self.emit(LOAD_CONST, none, 1)?;
                    }
                }
                let return_exclusion_start = if exception_unwind_start.is_none()
                    && finally_unwind_start.is_none()
                    && !self.control.active_exception_region_exclusions.is_empty()
                {
                    let start = self.assembler.label();
                    self.assembler.mark(start);
                    Some(start)
                } else {
                    None
                };
                if return_is_overridden {
                    if preserve_tos {
                        self.assembler
                            .set_location(self.source_location(statement.range));
                        self.emit(POP_TOP, 0, -1)?;
                    }
                } else {
                    self.emit(RETURN_VALUE, 0, -1)?;
                }
                if exception_unwind_start.is_some() || return_exclusion_start.is_some() {
                    let return_exclusion_end = self.assembler.label();
                    self.assembler.mark(return_exclusion_end);
                    let exclusion_start = exception_unwind_start
                        .or(return_exclusion_start)
                        .expect("return exclusion has a start");
                    for (index, exclusions) in self
                        .control
                        .active_exception_region_exclusions
                        .iter_mut()
                        .enumerate()
                    {
                        let exclusion_start = nested_finally_region_unwind_start
                            .filter(|(cutoff, _)| index >= *cutoff)
                            .map_or(exclusion_start, |(_, start)| start);
                        exclusions.push((exclusion_start, return_exclusion_end));
                    }
                }
                if !with_unwind_starts.is_empty() || finally_unwind_start.is_some() {
                    let unwind_end = self.assembler.label();
                    self.assembler.mark(unwind_end);
                    for (index, unwind_start) in with_unwind_starts {
                        self.control.active_with_region_exclusions[index]
                            .push((unwind_start, unwind_end));
                    }
                    if let Some(unwind_start) = finally_unwind_start {
                        for exclusions in &mut self.control.active_exception_region_exclusions {
                            exclusions.push((unwind_start, unwind_end));
                        }
                    }
                }
                self.set_depth(starting_depth);
            }
            Stmt::FunctionDef(definition) => self.compile_function_definition(definition)?,
            Stmt::ClassDef(definition) => self.compile_class_definition(definition)?,
            Stmt::Delete(statement) => {
                for target in &statement.targets {
                    self.compile_delete_target(target)?;
                }
            }
            Stmt::Assert(statement) => self.compile_assert(statement)?,
            Stmt::Raise(statement) => self.compile_raise(statement)?,
            Stmt::Import(statement) => self.compile_import(statement)?,
            Stmt::ImportFrom(statement) => self.compile_from_import(statement)?,
            Stmt::Global(_) | Stmt::Nonlocal(_) => {}
            Stmt::Try(statement) => self.compile_try(statement)?,
            Stmt::Match(statement) => self.compile_match(statement)?,
            Stmt::TypeAlias(statement) => self.compile_type_alias(statement)?,
            Stmt::With(statement) if statement.is_async => self.compile_async_with(statement)?,
            Stmt::With(statement) => self.compile_with(statement)?,
            Stmt::IpyEscapeCommand(_) => {
                return Err(unsupported(statement_name(statement)));
            }
        }

        if self.control.depth != starting_depth {
            return Err(CompileError::Internal(format!(
                "statement changed stack depth from {starting_depth} to {}",
                self.control.depth
            )));
        }
        Ok(())
    }

    fn compile_if(&mut self, statement: &ruff_python_ast::StmtIf) -> Result<(), CompileError> {
        if let Some(truthiness) = early_condition_truthiness(&statement.test)
            && statement.elif_else_clauses.len() <= 1
            && statement
                .elif_else_clauses
                .first()
                .is_none_or(|clause| clause.test.is_none())
        {
            if let Some(constant) = fold_constant(&statement.test) {
                self.add_constant(constant)?;
            }
            self.assembler
                .set_location(self.source_location(statement.test.range()));
            self.emit(NOP, 0, 0)?;
            if truthiness {
                let body_start = self.assembler.instruction_count();
                self.compile_suite(&statement.body)?;
                if self.assembler.instruction_count() == body_start
                    && let Some(Stmt::Pass(statement)) = statement.body.last()
                {
                    self.assembler
                        .set_location(self.source_location(statement.range()));
                    self.emit(NOP, 0, 0)?;
                }
                for clause in &statement.elif_else_clauses {
                    self.pre_register_suite_names(&clause.body)?;
                }
            } else {
                self.pre_register_suite_names(&statement.body)?;
                self.assembler.disable_load_fast_borrowing();
                if let Some(clause) = statement.elif_else_clauses.first() {
                    let body_start = self.assembler.instruction_count();
                    self.compile_suite(&clause.body)?;
                    if self.assembler.instruction_count() == body_start
                        && let Some(Stmt::Pass(statement)) = clause.body.last()
                    {
                        self.assembler
                            .set_location(self.source_location(statement.range()));
                        self.emit(NOP, 0, 0)?;
                    }
                }
            }
            return Ok(());
        }

        let base_depth = self.control.depth;
        let end = self.assembler.label();
        let mut branches: Vec<(Option<&Expr>, &[Stmt])> =
            vec![(Some(&statement.test), statement.body.as_slice())];
        branches.extend(
            statement
                .elif_else_clauses
                .iter()
                .map(|clause| (clause.test.as_ref(), clause.body.as_slice())),
        );

        let branch_count = branches.len();
        let has_terminating_else = branches
            .last()
            .is_some_and(|(test, body)| test.is_none() && suite_terminates(body));
        // The remaining branches still compile for symbol and constant-table side effects, but
        // their exception handlers do not contribute to CPython's stack size.
        let first_branch_is_always_true = jump_constant_truthiness(&statement.test) == Some(true);
        let mut first_branch_max_depth = None;
        for (branch_index, (test, body)) in branches.into_iter().enumerate() {
            if let Some(test) = test {
                let next = self.assembler.label();
                let exclude_not_taken = self.control.exclude_terminal_if_not_taken
                    || (!self.control.active_with_region_exclusions.is_empty()
                        && branch_index > 0
                        && suite_terminates(body));
                let retain_folded_test_in_protected_region = branch_index == 0
                    && early_condition_truthiness(test) == Some(true)
                    && (!self.control.active_with_region_exclusions.is_empty()
                        || !self.control.active_exception_region_exclusions.is_empty()
                        || self.control.generator_region_start.is_some());
                self.with_control_override(
                    |control| &mut control.exclude_terminal_if_not_taken,
                    exclude_not_taken,
                    |compiler| compiler.compile_jump_if(test, false, next),
                )?;
                if retain_folded_test_in_protected_region {
                    // CPython keeps the first folded branch marker inside the surrounding
                    // protected region instead of treating it as an artificial condition NOP.
                    if self.control.generator_region_start.is_some() {
                        self.control.generator_region_exclusions.pop();
                    }
                    for exclusions in &mut self.control.active_with_region_exclusions {
                        exclusions.pop();
                    }
                    for exclusions in &mut self.control.active_exception_region_exclusions {
                        exclusions.pop();
                    }
                }
                self.mark_definitely_evaluated_locals(test);
                let body_start = self.assembler.instruction_count();
                self.compile_with_strict_owned_loads(matches!(test, Expr::If(_)), |compiler| {
                    compiler.compile_suite(body)
                })?;
                let pass_only_body = matches!(body, [Stmt::Pass(_)]);
                if self.assembler.instruction_count() == body_start
                    && let Some(statement) = body.last()
                {
                    self.assembler
                        .set_location(self.source_location(statement.range()));
                    self.emit(NOP, 0, 0)?;
                }
                if branch_index == 0 && first_branch_is_always_true {
                    first_branch_max_depth = Some(self.control.max_depth);
                }
                if !suite_terminates(body) && branch_index + 1 < branch_count {
                    let nested_if_body_falls_through = matches!(
                        body.last(),
                        Some(Stmt::If(statement))
                            if early_condition_truthiness(&statement.test).is_none()
                                && !suite_terminates(&statement.body)
                    );
                    let nested_if_ends_in_break = matches!(
                        body.last(),
                        Some(Stmt::If(statement))
                            if matches!(statement.body.last(), Some(Stmt::Break(_)))
                    );
                    if nested_if_ends_in_break && has_terminating_else {
                        let Some(Stmt::If(statement)) = body.last() else {
                            unreachable!();
                        };
                        self.assembler
                            .set_location(self.source_location(statement.test.range()));
                    } else if nested_if_body_falls_through {
                        self.assembler.set_location(SourceLocation::NONE);
                    } else {
                        self.set_branch_end_location(body, body_start);
                    }
                    self.emit_jump_forward(JUMP_FORWARD, end, 0)?;
                    if nested_if_ends_in_break && has_terminating_else {
                        self.assembler.preserve_last_inlined_jump_nop();
                    } else if nested_if_body_falls_through {
                        self.assembler.preserve_last_no_location();
                    } else if pass_only_body {
                        self.assembler.preserve_last_direct_inlined_jump_nop();
                    } else if matches!(body.last(), Some(Stmt::If(_))) {
                        // CPython emits this jump in the nested if's empty join block. When
                        // small-exit inlining replaces it with a NOP, no preceding instruction
                        // in that block can make the NOP redundant.
                        self.assembler.preserve_last_direct_inlined_jump_nop();
                    }
                }
                self.assembler.mark(next);
                self.set_depth(base_depth);
            } else {
                let body_start = self.assembler.instruction_count();
                self.compile_suite(body)?;
                if branch_index + 1 == branch_count
                    && self.assembler.instruction_count() == body_start
                    && let Some(statement) = body.last()
                {
                    self.assembler
                        .set_location(self.source_location(statement.range()));
                    self.emit(NOP, 0, 0)?;
                }
            }
        }

        self.assembler.mark(end);
        self.set_depth(base_depth);
        if let Some(max_depth) = first_branch_max_depth {
            self.control.max_depth = max_depth;
        }
        Ok(())
    }

    pub(super) fn set_branch_end_location(&mut self, body: &[Stmt], body_start: usize) {
        if let Some(Stmt::If(statement)) = body.last()
            && let Some(test) = terminating_if_fallthrough_test(statement)
        {
            self.assembler
                .set_location(self.source_location(test.range()));
        } else if self.assembler.instruction_count() > body_start
            && let Some(location) = self.assembler.last_instruction_location()
        {
            self.assembler.set_location(location);
        } else if let Some(statement) = body.last() {
            self.assembler
                .set_location(self.source_location(statement.range()));
        }
    }

    pub(super) fn compile_with_strict_owned_loads<T>(
        &mut self,
        strict: bool,
        compile: impl FnOnce(&mut Self) -> Result<T, CompileError>,
    ) -> Result<T, CompileError> {
        let previous = strict.then(|| self.assembler.set_strict_owned_loads(true));
        let result = compile(self);
        if let Some(previous) = previous {
            self.assembler.set_strict_owned_loads(previous);
        }
        result
    }

    fn compile_loop_tail_suite(
        &mut self,
        body: &[Stmt],
        restart: Label,
    ) -> Result<bool, CompileError> {
        match body.split_last() {
            Some((Stmt::If(statement), leading)) => {
                self.compile_suite(leading)?;
                self.compile_loop_tail_if(statement, restart)?;
                Ok(true)
            }
            Some((Stmt::While(statement), leading))
                if statement.orelse.is_empty()
                    && early_condition_truthiness(&statement.test).is_none()
                    && !suite_contains_loop_break(&statement.body)
                    && !matches!(
                        statement.body.last(),
                        Some(Stmt::Try(statement)) if try_requires_loop_tail_inlining(statement)
                    ) =>
            {
                self.compile_suite(leading)?;
                self.compile_loop_tail_while(statement, restart)?;
                Ok(true)
            }
            Some((Stmt::Assert(statement), leading))
                if early_condition_truthiness(&statement.test).is_none() =>
            {
                self.compile_suite(leading)?;
                self.compile_loop_tail_assert(statement, restart)?;
                Ok(true)
            }
            _ => {
                self.compile_suite(body)?;
                Ok(false)
            }
        }
    }

    fn add_control_flow_region_exclusion(&mut self, start: Label, end: Label) {
        if self.control.generator_region_start.is_some() {
            self.control.generator_region_exclusions.push((start, end));
        }
        for exclusions in &mut self.control.active_with_region_exclusions {
            exclusions.push((start, end));
        }
        for exclusions in &mut self.control.active_exception_region_exclusions {
            exclusions.push((start, end));
        }
    }

    fn has_active_control_flow_region(&self) -> bool {
        self.control.generator_region_start.is_some()
            || !self.control.active_with_region_exclusions.is_empty()
            || !self.control.active_exception_region_exclusions.is_empty()
    }

    fn emit_active_with_unwind(
        &mut self,
        from_depth: usize,
        preserve_tos: bool,
    ) -> Result<Vec<(usize, Label)>, CompileError> {
        let context_count = self.control.active_with_exits.len();
        let mut unwind_starts = Vec::with_capacity(context_count - from_depth);
        for index in (from_depth..context_count).rev() {
            let context = self.control.active_with_exits[index];
            let start = self.assembler.label();
            self.assembler.mark(start);
            unwind_starts.push((index, start));
            self.assembler.set_location(context.location);
            if preserve_tos {
                self.emit(SWAP, 3, 0)?;
                self.emit(SWAP, 2, 0)?;
            }
            let none = self.add_constant(Constant::None)?;
            self.emit(LOAD_CONST, none, 1)?;
            self.emit(LOAD_CONST, none, 1)?;
            self.emit(LOAD_CONST, none, 1)?;
            self.emit(CALL, 3, -4)?;
            if context.is_async {
                self.compile_awaitable_on_stack(2)?;
            }
            self.emit(POP_TOP, 0, -1)?;
        }
        Ok(unwind_starts)
    }

    fn emit_loop_iterator_cleanup(
        &mut self,
        from_depth: usize,
        to_depth: usize,
        preserve_tos: bool,
    ) -> Result<(), CompileError> {
        for index in (from_depth..to_depth).rev() {
            match self.control.loops[index].iterator_cleanup {
                IteratorCleanup::None => {}
                IteratorCleanup::Sync | IteratorCleanup::Async => {
                    if preserve_tos {
                        self.emit(SWAP, 2, 0)?;
                    }
                    self.emit(POP_TOP, 0, -1)?;
                }
            }
        }
        Ok(())
    }

    fn emit_pass_finally_nops(&mut self) -> Result<(), CompileError> {
        for index in (0..self.control.active_pass_finally_locations.len()).rev() {
            let location = self.control.active_pass_finally_locations[index];
            self.assembler.set_location(location);
            self.emit(NOP, 0, 0)?;
        }
        Ok(())
    }

    fn emit_loop_control_exception_unwind(
        &mut self,
        loop_depth: usize,
    ) -> Result<(), CompileError> {
        for index in (0..self.control.active_exception_handlers.len()).rev() {
            let (handler_loop_depth, name) = {
                let handler = &self.control.active_exception_handlers[index];
                (handler.loop_depth, handler.name.clone())
            };
            if handler_loop_depth < loop_depth {
                break;
            }
            self.emit(POP_EXCEPT, 0, -1)?;
            if let Some(name) = &name {
                let none = self.add_constant(Constant::None)?;
                self.emit(LOAD_CONST, none, 1)?;
                self.store_name(name)?;
                self.delete_name(name)?;
            }
        }
        Ok(())
    }

    fn emit_finally_end_unwind(
        &mut self,
        from_depth: usize,
        preserve_tos: bool,
    ) -> Result<Option<Label>, CompileError> {
        let mut unwind_start = None;
        for _ in from_depth..self.control.active_finally_end_blocks {
            if preserve_tos {
                self.emit(SWAP, 2, 0)?;
            }
            self.emit(POP_TOP, 0, -1)?;
            if preserve_tos {
                self.emit(SWAP, 2, 0)?;
            }
            if unwind_start.is_none() {
                let start = self.assembler.label();
                self.assembler.mark(start);
                unwind_start = Some(start);
            }
            self.emit(POP_EXCEPT, 0, -1)?;
        }
        Ok(unwind_start)
    }

    fn compile_loop_tail_while(
        &mut self,
        statement: &ruff_python_ast::StmtWhile,
        restart: Label,
    ) -> Result<(), CompileError> {
        let base_depth = self.control.depth;
        let start = self.assembler.label();
        let body_label = self.assembler.label();
        let control_region = self
            .has_active_control_flow_region()
            .then(|| (self.assembler.label(), self.assembler.label()));

        self.assembler.mark(start);
        self.compile_jump_if(&statement.test, true, body_label)?;
        if let Some((control_start, _)) = control_region {
            self.assembler
                .mark_before_trailing_instructions(control_start, 2);
        }
        self.assembler
            .set_location(self.source_location(statement.test.range()));
        self.emit_jump_backward(JUMP_BACKWARD, restart, 0)?;
        if let Some((control_start, control_end)) = control_region {
            self.assembler.mark(control_end);
            self.add_control_flow_region_exclusion(control_start, control_end);
        }

        self.assembler.mark(body_label);
        self.set_depth(base_depth);
        let context = self.loop_context(start, restart, IteratorCleanup::None, false, false);
        let body_start = self.assembler.instruction_count();
        let tail_jumps = self.with_loop_context(context, |compiler| {
            compiler.with_control_override(
                |control| &mut control.exclude_loop_tail_not_taken_from_control_flow_regions,
                control_region.is_some(),
                |compiler| compiler.compile_while_tail_suite(&statement.body, start),
            )
        })?;
        if !tail_jumps && !suite_terminates(&statement.body) {
            self.emit_while_backedge(&statement.body, body_start, start)?;
        }
        Ok(())
    }

    fn compile_while_tail_suite(
        &mut self,
        body: &[Stmt],
        restart: Label,
    ) -> Result<bool, CompileError> {
        if let Some((Stmt::Try(statement), leading)) = body.split_last()
            && try_requires_loop_tail_inlining(statement)
        {
            self.compile_suite(leading)?;
            self.compile_try_inner(statement, false, true, true)?;
            Ok(false)
        } else {
            self.compile_loop_tail_suite(body, restart)
        }
    }

    fn emit_while_backedge(
        &mut self,
        body: &[Stmt],
        body_start: usize,
        restart: Label,
    ) -> Result<(), CompileError> {
        let inline_try_tail = matches!(
            body.last(),
            Some(Stmt::Try(statement)) if try_requires_loop_tail_inlining(statement)
        );
        if !inline_try_tail {
            self.set_branch_end_location(body, body_start);
        }
        self.emit_jump_backward(JUMP_BACKWARD, restart, 0)?;
        if inline_try_tail {
            self.assembler.prepare_last_no_location_block_for_inlining();
        }
        Ok(())
    }

    fn compile_loop_tail_assert(
        &mut self,
        statement: &ruff_python_ast::StmtAssert,
        restart: Label,
    ) -> Result<(), CompileError> {
        let base_depth = self.control.depth;
        let failure = self.assembler.label();
        let exclude_not_taken = !self.control.active_with_region_exclusions.is_empty();
        self.with_control_override(
            |control| &mut control.exclude_terminal_if_not_taken,
            exclude_not_taken,
            |compiler| {
                if let Expr::BoolOp(boolean) = statement.test.as_ref()
                    && boolean.op == BoolOp::Or
                    && let Some((last, leading)) = boolean.values.split_last()
                {
                    for value in leading {
                        let next = compiler.assembler.label();
                        compiler.compile_jump_if(value, false, next)?;
                        compiler
                            .assembler
                            .set_location(compiler.source_location(value.range()));
                        compiler.emit_jump_backward(JUMP_BACKWARD, restart, 0)?;
                        compiler.assembler.mark(next);
                    }
                    compiler.compile_jump_if(last, false, failure)?;
                    compiler
                        .assembler
                        .set_location(compiler.source_location(last.range()));
                } else {
                    compiler.compile_jump_if(&statement.test, false, failure)?;
                    compiler
                        .assembler
                        .set_location(compiler.source_location(statement.test.range()));
                }
                Ok(())
            },
        )?;
        self.emit_jump_backward(JUMP_BACKWARD, restart, 0)?;

        self.assembler.mark(failure);
        self.set_depth(base_depth);
        self.assembler
            .set_location(self.source_location(statement.range));
        self.emit_common_constant(CommonConstant::ASSERTIONERROR)?;
        if let Some(message) = &statement.msg {
            self.compile_expression(message)?;
            self.assembler
                .set_location(self.source_location(statement.range));
            self.emit(CALL, 0, -1)?;
        }
        self.assembler
            .set_location(self.source_location(statement.test.range()));
        self.emit(RAISE_VARARGS, 1, -1)?;
        Ok(())
    }

    fn compile_loop_tail_if(
        &mut self,
        statement: &ruff_python_ast::StmtIf,
        restart: Label,
    ) -> Result<(), CompileError> {
        let base_depth = self.control.depth;
        if statement.elif_else_clauses.is_empty() {
            if self.control.generator_region_start.is_none()
                && let Expr::Compare(comparison) = statement.test.as_ref()
                && comparison.ops.len() > 1
            {
                return self.compile_loop_tail_chained_compare(statement, comparison, restart);
            }
            let body = statement.body.as_slice();
            let body_label = self.assembler.label();
            self.compile_loop_tail_condition(&statement.test, true, body_label, restart)?;
            self.assembler.mark(body_label);
            self.set_depth(base_depth);
            let body_start = self.assembler.instruction_count();
            let nested_tail = self.compile_with_strict_owned_loads(
                matches!(statement.test.as_ref(), Expr::If(_)),
                |compiler| compiler.compile_loop_tail_suite(body, restart),
            )?;
            if !nested_tail && !suite_terminates(body) {
                self.set_branch_end_location(body, body_start);
                self.emit_jump_backward(JUMP_BACKWARD, restart, 0)?;
            }
            return Ok(());
        }
        let mut branches: Vec<(Option<&Expr>, &[Stmt])> =
            vec![(Some(&statement.test), statement.body.as_slice())];
        branches.extend(
            statement
                .elif_else_clauses
                .iter()
                .map(|clause| (clause.test.as_ref(), clause.body.as_slice())),
        );
        let branch_count = branches.len();
        let has_else = branches.last().is_some_and(|(test, _)| test.is_none());

        for (branch_index, (test, body)) in branches.into_iter().enumerate() {
            if !has_else && branch_index + 1 == branch_count {
                let test = test.expect("last branch without else has a test");
                let body_label = self.assembler.label();
                let control_region = (early_condition_truthiness(test).is_none()
                    && self.has_active_control_flow_region())
                .then(|| (self.assembler.label(), self.assembler.label()));
                let generator_exclusion = (control_region.is_none()
                    && self.control.generator_region_start.is_some())
                .then(|| self.assembler.label());
                self.compile_jump_if(test, true, body_label)?;
                if let Some((control_start, _)) = control_region {
                    self.assembler
                        .mark_before_trailing_instructions(control_start, 2);
                }
                if let Some(exclusion_start) = generator_exclusion {
                    self.assembler
                        .mark_before_trailing_instructions(exclusion_start, 2);
                }
                self.emit_jump_backward(JUMP_BACKWARD, restart, 0)?;
                if let Some((control_start, control_end)) = control_region {
                    self.assembler.mark(control_end);
                    self.add_control_flow_region_exclusion(control_start, control_end);
                }
                self.assembler.mark(body_label);
                if let Some(exclusion_start) = generator_exclusion {
                    self.control
                        .generator_region_exclusions
                        .push((exclusion_start, body_label));
                }
                self.set_depth(base_depth);
                let body_start = self.assembler.instruction_count();
                let nested_tail = self
                    .compile_with_strict_owned_loads(matches!(test, Expr::If(_)), |compiler| {
                        compiler.compile_loop_tail_suite(body, restart)
                    })?;
                if !nested_tail && !suite_terminates(body) {
                    self.set_branch_end_location(body, body_start);
                    self.emit_jump_backward(JUMP_BACKWARD, restart, 0)?;
                }
                return Ok(());
            }
            let next = test.map(|_| self.assembler.label());
            if let (Some(test), Some(next)) = (test, next) {
                let exclude_from_control_flow_regions =
                    !self.control.active_exception_region_exclusions.is_empty()
                        || self
                            .control
                            .exclude_loop_tail_not_taken_from_control_flow_regions;
                let exclude_from_all_exception_regions =
                    !self.control.active_exception_region_exclusions.is_empty();
                self.with_control_override(
                    |control| &mut control.exclude_condition_not_taken_from_exception,
                    exclude_from_control_flow_regions,
                    |compiler| {
                        compiler.with_control_override(
                            |control| {
                                &mut control.exclude_condition_not_taken_from_all_exception_regions
                            },
                            exclude_from_all_exception_regions,
                            |compiler| compiler.compile_jump_if(test, false, next),
                        )
                    },
                )?;
            }
            let nested_tail = self.compile_with_strict_owned_loads(
                test.is_some_and(|test| matches!(test, Expr::If(_))),
                |compiler| compiler.compile_loop_tail_suite(body, restart),
            )?;
            if !nested_tail && !suite_terminates(body) {
                if let Some(location) = self.assembler.last_instruction_location() {
                    self.assembler.set_location(location);
                } else if let Some(statement) = body.last() {
                    self.assembler
                        .set_location(self.source_location(statement.range()));
                }
                self.emit_jump_backward(JUMP_BACKWARD, restart, 0)?;
            }
            if let Some(next) = next {
                self.assembler.mark(next);
                self.set_depth(base_depth);
            }
        }
        if !has_else {
            self.assembler
                .set_location(self.source_location(statement.test.range()));
            self.emit_jump_backward(JUMP_BACKWARD, restart, 0)?;
        }
        Ok(())
    }

    fn compile_loop_tail_condition(
        &mut self,
        expression: &Expr,
        jump_on: bool,
        success: Label,
        restart: Label,
    ) -> Result<(), CompileError> {
        if let Expr::UnaryOp(unary) = expression
            && unary.op == UnaryOp::Not
        {
            return self.compile_loop_tail_condition(&unary.operand, !jump_on, success, restart);
        }
        if let Expr::BoolOp(boolean) = expression {
            let Some((last, leading)) = boolean.values.split_last() else {
                return Err(CompileError::Internal(
                    "boolean expression contains no values".to_string(),
                ));
            };
            let short_circuit_value = boolean.op == BoolOp::Or;
            if short_circuit_value == jump_on {
                for value in leading {
                    self.compile_jump_if(value, jump_on, success)?;
                }
            } else {
                for value in leading {
                    let next = self.assembler.label();
                    self.compile_loop_tail_condition(value, !short_circuit_value, next, restart)?;
                    self.assembler.mark(next);
                }
            }
            return self.compile_loop_tail_condition(last, jump_on, success, restart);
        }

        let control_region = (early_condition_truthiness(expression).is_none()
            && self.has_active_control_flow_region())
        .then(|| (self.assembler.label(), self.assembler.label()));
        let generator_exclusion = (control_region.is_none()
            && self.control.generator_region_start.is_some())
        .then(|| self.assembler.label());
        self.compile_jump_if(expression, jump_on, success)?;
        if let Some((control_start, _)) = control_region {
            self.assembler
                .mark_before_trailing_instructions(control_start, 2);
        }
        if let Some(exclusion_start) = generator_exclusion {
            self.assembler
                .mark_before_trailing_instructions(exclusion_start, 2);
        }
        self.emit_jump_backward(JUMP_BACKWARD, restart, 0)?;
        if let Some((control_start, control_end)) = control_region {
            self.assembler.mark(control_end);
            self.add_control_flow_region_exclusion(control_start, control_end);
        }
        if let Some(exclusion_start) = generator_exclusion {
            self.control
                .generator_region_exclusions
                .push((exclusion_start, success));
        }
        Ok(())
    }

    fn compile_loop_tail_chained_compare(
        &mut self,
        statement: &ruff_python_ast::StmtIf,
        comparison: &ruff_python_ast::ExprCompare,
        restart: Label,
    ) -> Result<(), CompileError> {
        let base_depth = self.control.depth;
        let cleanup = self.assembler.label();
        let success = self.assembler.label();
        let body = self.assembler.label();

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
        self.emit_jump_forward(POP_JUMP_IF_TRUE, success, -1)?;
        self.emit(NOT_TAKEN, 0, 0)?;
        self.emit_jump_backward(JUMP_BACKWARD, restart, 0)?;

        self.assembler.mark(success);
        self.set_depth(base_depth);
        self.assembler
            .set_location(self.source_location(comparison.range));
        self.emit_jump_forward(JUMP_FORWARD, body, 0)?;

        self.assembler.mark(cleanup);
        self.set_depth(base_depth + 1);
        self.assembler
            .set_location(self.source_location(comparison.range));
        self.emit(POP_TOP, 0, -1)?;
        self.emit_jump_backward(JUMP_BACKWARD, restart, 0)?;

        self.assembler.mark(body);
        self.set_depth(base_depth);
        let body_start = self.assembler.instruction_count();
        let nested_tail = self.compile_loop_tail_suite(&statement.body, restart)?;
        if !nested_tail && !suite_terminates(&statement.body) {
            self.set_branch_end_location(&statement.body, body_start);
            self.emit_jump_backward(JUMP_BACKWARD, restart, 0)?;
        }
        Ok(())
    }

    fn compile_while(
        &mut self,
        statement: &ruff_python_ast::StmtWhile,
    ) -> Result<(), CompileError> {
        let base_depth = self.control.depth;
        if let Some(truthiness) = early_condition_truthiness(&statement.test) {
            if self.output.constants.is_empty()
                && let Some(constant) = fold_constant(&statement.test)
            {
                self.add_constant(constant)?;
            }
            if !truthiness {
                self.pre_register_suite_names(&statement.body)?;
                // The eliminated loop body leaves a CFG boundary that CPython's borrowed-load
                // optimizer does not traverse into the following block.
                self.assembler.set_strict_owned_loads(true);
                self.assembler
                    .set_location(self.source_location(statement.test.range()));
                self.emit(NOP, 0, 0)?;
                self.compile_suite(&statement.orelse)?;
                return Ok(());
            }

            let start = self.assembler.label();
            let end = self.assembler.label();
            self.assembler.mark(start);
            self.assembler
                .set_location(self.source_location(statement.test.range()));
            self.emit(NOP, 0, 0)?;
            let context = self.loop_context(
                start,
                end,
                IteratorCleanup::None,
                false,
                self.control.preserve_finally_break_exit_loop_range == Some(statement.range),
            );
            let body_start = self.assembler.instruction_count();
            let tail_jumps = self.with_loop_context(context, |compiler| {
                compiler.compile_while_tail_suite(&statement.body, start)
            })?;
            if !tail_jumps && !suite_terminates(&statement.body) {
                self.emit_while_backedge(&statement.body, body_start, start)?;
            }
            self.pre_register_suite_names(&statement.orelse)?;
            self.assembler.mark(end);
            if suite_contains_loop_break(&statement.body) {
                // A constant-true loop can reach its exit only through `break`. CPython removes
                // a redundant break jump before borrowed-load analysis but does not reconnect
                // the large exit block; a copied small exit remains independently reachable.
                self.assembler.prevent_borrow_reachability(end);
            }
            self.set_depth(base_depth);
            if statement.orelse.is_empty() {
                self.assembler
                    .set_location(self.source_location(statement.test.range()));
            }
            return Ok(());
        }
        let start = self.assembler.label();
        let else_label = self.assembler.label();
        let end = self.assembler.label();

        self.assembler.mark(start);
        let exclude_not_taken = !self.control.active_exception_region_exclusions.is_empty()
            || self.control.active_terminal_withs > 0
            || self.control.generator_region_start.is_some();
        self.with_control_override(
            |control| &mut control.exclude_condition_not_taken_from_exception,
            exclude_not_taken,
            |compiler| compiler.compile_jump_if(&statement.test, false, else_label),
        )?;
        let context = self.loop_context(
            start,
            end,
            IteratorCleanup::None,
            false,
            self.control.preserve_finally_break_exit_loop_range == Some(statement.range),
        );
        let body_start = self.assembler.instruction_count();
        let tail_jumps = self.with_loop_context(context, |compiler| {
            compiler.compile_while_tail_suite(&statement.body, start)
        })?;
        if !tail_jumps && !suite_terminates(&statement.body) {
            self.emit_while_backedge(&statement.body, body_start, start)?;
        }

        self.assembler.mark(else_label);
        self.set_depth(base_depth);
        if statement.orelse.is_empty() && !self.control.active_with_region_exclusions.is_empty() {
            let exclusion_start = self.assembler.label();
            self.assembler.mark(exclusion_start);
            self.assembler
                .set_location(self.source_location(statement.test.range()));
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
        self.compile_suite(&statement.orelse)?;
        self.assembler.mark(end);
        self.set_depth(base_depth);
        if statement.orelse.is_empty() {
            self.assembler
                .set_location(self.source_location(statement.test.range()));
        }
        Ok(())
    }

    fn compile_for(&mut self, statement: &ruff_python_ast::StmtFor) -> Result<(), CompileError> {
        let base_depth = self.control.depth;
        let has_break = suite_contains_loop_break(&statement.body);
        let start = self.assembler.label();
        let cleanup = self.assembler.label();
        let end = self.assembler.label();
        let mut newly_initialized_targets = Vec::new();
        collect_target_names(&statement.target, &mut newly_initialized_targets);
        newly_initialized_targets.retain(|name| !self.is_initialized(name));

        self.compile_iterable_expression(&statement.iter)?;
        self.assembler
            .set_location(self.source_location(statement.iter.range()));
        self.emit(GET_ITER, 0, 0)?;
        self.assembler.mark(start);
        self.emit_jump_forward(FOR_ITER, cleanup, 1)?;
        self.compile_store_target(&statement.target)?;

        let context = self.loop_context(
            start,
            end,
            IteratorCleanup::Sync,
            false,
            self.control.preserve_finally_break_exit_loop_range == Some(statement.range),
        );
        let body_start = self.assembler.instruction_count();
        let tail_jumps = self.with_loop_context(context, |compiler| {
            compiler.compile_loop_tail_suite(&statement.body, start)
        })?;
        if !tail_jumps && !suite_terminates(&statement.body) {
            if self.assembler.instruction_count() > body_start
                && let Some(location) = self.assembler.last_instruction_location()
            {
                self.assembler.set_location(location);
            } else if let Some(statement) = statement.body.last() {
                self.assembler
                    .set_location(self.source_location(statement.range()));
            }
            self.emit_jump_backward(JUMP_BACKWARD, start, 0)?;
        }

        self.assembler.mark(cleanup);
        self.set_depth(base_depth + 2);
        self.assembler
            .set_location(self.source_location(statement.iter.range()));
        self.emit(END_FOR, 0, -1)?;
        self.emit(POP_ITER, 0, -1)?;
        for name in &newly_initialized_targets {
            let name = self.required_name_id(name);
            self.symbols.initialized_locals.remove(name);
        }
        let orelse_start = self.assembler.instruction_count();
        self.compile_suite(&statement.orelse)?;
        if self.assembler.instruction_count() == orelse_start
            && let [Stmt::Pass(statement)] = statement.orelse.as_slice()
            && self.control.active_exception_region_exclusions.is_empty()
            && !self.control.active_with_region_exclusions.is_empty()
        {
            let noop_start = self.assembler.label();
            self.assembler.mark(noop_start);
            self.assembler
                .set_location(self.source_location(statement.range));
            self.emit(NOP, 0, 0)?;
            let noop_end = self.assembler.label();
            self.assembler.mark(noop_end);
            for exclusions in &mut self.control.active_with_region_exclusions {
                exclusions.push((noop_start, noop_end));
            }
        }
        if has_break {
            self.assembler.mark(end);
        }
        self.set_depth(base_depth);
        Ok(())
    }

    fn compile_async_for(
        &mut self,
        statement: &ruff_python_ast::StmtFor,
    ) -> Result<(), CompileError> {
        if self.unit.flags & (CO_COROUTINE | CO_ASYNC_GENERATOR) == 0 {
            return Err(unsupported("async for outside coroutine code"));
        }

        let base_depth = self.control.depth;
        let has_break = suite_contains_loop_break(&statement.body);
        let start = self.assembler.label();
        let protected_start = self.assembler.label();
        let yielded = self.assembler.label();
        let yielded_end = self.assembler.label();
        let send = self.assembler.label();
        let send_end = self.assembler.label();
        let protected_end = self.assembler.label();
        let cleanup_throw = self.assembler.label();
        let async_cleanup = self.assembler.label();
        let end = self.assembler.label();
        let mut newly_initialized_targets = Vec::new();
        collect_target_names(&statement.target, &mut newly_initialized_targets);
        newly_initialized_targets.retain(|name| !self.is_initialized(name));

        self.compile_expression(&statement.iter)?;
        let iterator_location = self.source_location(statement.iter.range());
        self.assembler.set_location(iterator_location);
        self.emit(GET_AITER, 0, 0)?;
        self.assembler
            .set_location(self.source_location(statement.range));
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
        self.set_depth(base_depth + 3);
        self.emit(END_SEND, 0, -1)?;
        self.assembler.mark(protected_end);
        self.emit(NOT_TAKEN, 0, 0)?;
        self.compile_store_target(&statement.target)?;

        let context = self.loop_context(
            start,
            end,
            IteratorCleanup::Async,
            false,
            self.control.preserve_finally_break_exit_loop_range == Some(statement.range),
        );
        let body_start = self.assembler.instruction_count();
        let tail_jumps = self.with_loop_context(context, |compiler| {
            compiler.compile_loop_tail_suite(&statement.body, start)
        })?;
        if !tail_jumps && !suite_terminates(&statement.body) {
            if self.assembler.instruction_count() > body_start
                && let Some(location) = self.assembler.last_instruction_location()
            {
                self.assembler.set_location(location);
            }
            self.emit_jump_backward(JUMP_BACKWARD, start, 0)?;
        }

        self.assembler.mark(cleanup_throw);
        self.set_depth(base_depth + 4);
        self.assembler
            .set_location(self.source_location(statement.range));
        self.emit(CLEANUP_THROW, 0, -1)?;
        let cleanup_throw_end = self.assembler.label();
        self.assembler.mark(cleanup_throw_end);
        self.emit_jump_backward(JUMP_BACKWARD_NO_INTERRUPT, send_end, 0)?;
        self.assembler.add_exception_region(
            yielded,
            yielded_end,
            cleanup_throw,
            (base_depth + 3).cast_unsigned(),
            false,
        );

        self.assembler.mark(async_cleanup);
        self.control
            .generator_region_exclusions
            .push((cleanup_throw, async_cleanup));
        for exclusions in &mut self.control.active_with_region_exclusions {
            exclusions.push((cleanup_throw_end, async_cleanup));
        }
        for exclusions in &mut self.control.active_exception_region_exclusions {
            exclusions.push((cleanup_throw_end, async_cleanup));
        }
        self.set_depth(base_depth + 2);
        self.assembler.set_location(iterator_location);
        self.apply_stack_effect(-2)?;
        self.assembler.emit_backward(END_ASYNC_FOR, send_end);
        self.assembler.add_exception_region(
            protected_start,
            protected_end,
            async_cleanup,
            (base_depth + 1).cast_unsigned(),
            false,
        );
        self.assembler.add_exception_region(
            cleanup_throw,
            cleanup_throw_end,
            async_cleanup,
            (base_depth + 1).cast_unsigned(),
            false,
        );
        for name in &newly_initialized_targets {
            let name = self.required_name_id(name);
            self.symbols.initialized_locals.remove(name);
        }
        self.compile_suite(&statement.orelse)?;
        if has_break {
            self.assembler.mark(end);
        }
        if self.control.emit_protected_async_for_end_nop
            && statement.orelse.is_empty()
            && !has_break
        {
            let noop_start = self.assembler.label();
            self.assembler.mark(noop_start);
            self.assembler
                .set_location(self.source_location(statement.iter.range()));
            self.emit(NOP, 0, 0)?;
            let noop_end = self.assembler.label();
            self.assembler.mark(noop_end);
            // CPython's coroutine stop handler excludes both the synthetic end block and the
            // preceding END_ASYNC_FOR, while the surrounding protected region still owns the
            // END_ASYNC_FOR itself.
            self.control
                .generator_region_exclusions
                .push((async_cleanup, noop_end));
            for exclusions in &mut self.control.active_with_region_exclusions {
                exclusions.push((noop_start, noop_end));
            }
            for exclusions in &mut self.control.active_exception_region_exclusions {
                exclusions.push((noop_start, noop_end));
            }
        }
        self.set_depth(base_depth);
        Ok(())
    }

    fn compile_augmented_assignment(
        &mut self,
        assignment: &ruff_python_ast::StmtAugAssign,
    ) -> Result<(), CompileError> {
        match assignment.target.as_ref() {
            Expr::Name(target) => {
                let target_location = self.source_location(target.range);
                self.assembler.set_location(target_location);
                self.load_name(target.id.as_str())?;
                self.compile_expression(&assignment.value)?;
                self.assembler
                    .set_location(self.source_location(assignment.range));
                self.emit_binary_operation(binary_operator(assignment.op, true))?;
                self.assembler.set_location(target_location);
                self.store_name(target.id.as_str())?;
            }
            Expr::Attribute(target) => {
                self.compile_expression(&target.value)?;
                self.assembler
                    .set_location(self.source_location(target.range));
                self.emit(COPY, 1, 1)?;
                let index = self.name_index(target.attr.as_str())?;
                let attribute_location = self.source_location(self.attribute_opcode_range(target));
                self.assembler.set_location(attribute_location);
                self.emit(LOAD_ATTR, index << 1, 0)?;
                self.compile_expression(&assignment.value)?;
                self.assembler
                    .set_location(self.source_location(assignment.range));
                self.emit_binary_operation(binary_operator(assignment.op, true))?;
                self.assembler.set_location(attribute_location);
                self.emit(SWAP, 2, 0)?;
                self.emit(STORE_ATTR, index, -2)?;
            }
            Expr::Subscript(target) => {
                self.compile_expression(&target.value)?;
                let optimized_slice = if let Expr::Slice(slice) = target.slice.as_ref()
                    && two_element_slice_optimization(slice)
                {
                    let slice_location = self.source_location(slice.range);
                    self.compile_optional_slice_bound(slice.lower.as_deref(), slice_location)?;
                    self.compile_optional_slice_bound(slice.upper.as_deref(), slice_location)?;
                    true
                } else {
                    self.compile_expression(&target.slice)?;
                    false
                };
                let target_location = self.source_location(target.range);
                self.assembler.set_location(target_location);
                if optimized_slice {
                    self.emit(COPY, 3, 1)?;
                    self.emit(COPY, 3, 1)?;
                    self.emit(COPY, 3, 1)?;
                    self.emit(BINARY_SLICE, 0, -2)?;
                } else {
                    self.emit(COPY, 2, 1)?;
                    self.emit(COPY, 2, 1)?;
                    self.emit_binary_operation(BinaryOperation::SUBSCR)?;
                }
                self.compile_expression(&assignment.value)?;
                self.assembler
                    .set_location(self.source_location(assignment.range));
                self.emit_binary_operation(binary_operator(assignment.op, true))?;
                self.assembler.set_location(target_location);
                if optimized_slice {
                    self.emit(SWAP, 4, 0)?;
                    self.emit(SWAP, 3, 0)?;
                    self.emit(SWAP, 2, 0)?;
                    self.emit(STORE_SLICE, 0, -4)?;
                } else {
                    self.emit(SWAP, 3, 0)?;
                    self.emit(SWAP, 2, 0)?;
                    self.emit(STORE_SUBSCR, 0, -3)?;
                }
            }
            _ => return Err(unsupported("augmented assignment target")),
        }
        Ok(())
    }

    fn compile_annotated_assignment(
        &mut self,
        assignment: &ruff_python_ast::StmtAnnAssign,
    ) -> Result<(), CompileError> {
        if assignment.simple
            && (matches!(self.unit.scope, Scope::Module)
                || matches!(self.unit.scope, Scope::Class { .. })
                    && self.unit.flags & CO_FUTURE_ANNOTATIONS != 0)
        {
            if let Some(value) = &assignment.value {
                self.compile_expression(value)?;
                self.compile_store_target(&assignment.target)?;
            }
            let Expr::Name(target) = assignment.target.as_ref() else {
                return Err(unsupported("simple annotation target"));
            };
            if self.unit.flags & CO_FUTURE_ANNOTATIONS != 0 {
                self.assembler
                    .set_location(self.source_location(assignment.annotation.range()));
                let annotation = unparse_annotation(&assignment.annotation);
                self.load_string_constant(&annotation)?;
                self.assembler
                    .set_location(self.source_location(assignment.range));
                self.load_name("__annotations__")?;
                let target = self.mangled_name(target.id.as_str());
                self.load_string_constant(&target)?;
                self.emit(STORE_SUBSCR, 0, -3)?;
            } else {
                let index = self.unit.module_annotation_index;
                self.unit.module_annotation_index += 1;
                if self.output.constants.is_empty() {
                    self.add_constant(Constant::Int(u64::from(index)))?;
                }
                self.assembler
                    .set_location(self.source_location(assignment.range));
                self.load_name("__conditional_annotations__")?;
                self.emit(LOAD_SMALL_INT, index, 1)?;
                self.emit(SET_ADD, 1, -1)?;
                self.emit(POP_TOP, 0, -1)?;
            }
            return Ok(());
        }
        if let Some(value) = &assignment.value {
            self.compile_expression(value)?;
            self.compile_store_target(&assignment.target)?;
            return Ok(());
        }

        match assignment.target.as_ref() {
            Expr::Name(_) => {}
            Expr::Attribute(attribute) => {
                self.compile_expression(&attribute.value)?;
                self.assembler
                    .set_location(self.source_location(attribute.value.range()));
                self.emit(POP_TOP, 0, -1)?;
            }
            Expr::Subscript(subscript) => {
                self.compile_expression(&subscript.value)?;
                self.assembler
                    .set_location(self.source_location(subscript.value.range()));
                self.emit(POP_TOP, 0, -1)?;
                self.compile_discarded_annotation_slice(&subscript.slice)?;
            }
            _ => return Err(unsupported("annotated assignment target")),
        }
        Ok(())
    }

    fn compile_discarded_annotation_slice(
        &mut self,
        expression: &Expr,
    ) -> Result<(), CompileError> {
        match expression {
            Expr::Tuple(tuple) => {
                for element in &tuple.elts {
                    self.compile_discarded_annotation_slice(element)?;
                }
            }
            Expr::Slice(slice) => {
                for element in [
                    slice.lower.as_deref(),
                    slice.upper.as_deref(),
                    slice.step.as_deref(),
                ]
                .into_iter()
                .flatten()
                {
                    self.compile_discarded_annotation_slice(element)?;
                }
            }
            _ if fold_constant(expression).is_some() => {
                if let Some(constant) = first_literal_constant(expression) {
                    self.add_constant(constant)?;
                }
            }
            _ => {
                self.compile_expression(expression)?;
                self.assembler
                    .set_location(self.source_location(expression.range()));
                self.emit(POP_TOP, 0, -1)?;
            }
        }
        Ok(())
    }

    fn compile_assert(
        &mut self,
        statement: &ruff_python_ast::StmtAssert,
    ) -> Result<(), CompileError> {
        if early_condition_truthiness(&statement.test) == Some(true) {
            if let Some(constant) = fold_constant(&statement.test) {
                self.add_constant(constant)?;
            }
            if let Some(message) = &statement.msg {
                self.pre_register_expression_names(message)?;
                self.pre_register_expression_constants(message)?;
            }
            self.assembler
                .set_location(self.source_location(statement.test.range()));
            return self.emit(NOP, 0, 0);
        }
        let base_depth = self.control.depth;
        let end = self.assembler.label();
        let exclude_not_taken = !self.control.active_with_region_exclusions.is_empty();
        self.with_control_override(
            |control| &mut control.exclude_terminal_if_not_taken,
            exclude_not_taken,
            |compiler| compiler.compile_jump_if(&statement.test, true, end),
        )?;
        self.assembler
            .set_location(self.source_location(statement.range));
        self.emit_common_constant(CommonConstant::ASSERTIONERROR)?;
        if let Some(message) = &statement.msg {
            self.compile_expression(message)?;
            self.assembler
                .set_location(self.source_location(statement.range));
            self.emit(CALL, 0, -1)?;
        }
        self.assembler
            .set_location(self.source_location(statement.test.range()));
        self.emit(RAISE_VARARGS, 1, -1)?;
        self.assembler.mark(end);
        self.set_depth(base_depth);
        Ok(())
    }

    fn compile_raise(
        &mut self,
        statement: &ruff_python_ast::StmtRaise,
    ) -> Result<(), CompileError> {
        let mut argument_count = 0;
        if let Some(exception) = &statement.exc {
            self.compile_expression(exception)?;
            argument_count += 1;
            if let Some(cause) = &statement.cause {
                self.compile_expression(cause)?;
                argument_count += 1;
            }
        }
        self.emit(RAISE_VARARGS, argument_count, -argument_count.cast_signed())
    }

    fn compile_try(&mut self, statement: &ruff_python_ast::StmtTry) -> Result<(), CompileError> {
        self.compile_try_inner(statement, false, true, false)
    }

    fn compile_try_inner(
        &mut self,
        statement: &ruff_python_ast::StmtTry,
        terminal: bool,
        emit_statement_nop: bool,
        exclude_terminal_body_not_taken: bool,
    ) -> Result<(), CompileError> {
        if !statement.finalbody.is_empty() {
            return self.compile_try_finally(statement, terminal);
        }
        self.compile_try_except(
            statement,
            terminal,
            emit_statement_nop,
            exclude_terminal_body_not_taken,
        )
    }

    fn compile_try_except(
        &mut self,
        statement: &ruff_python_ast::StmtTry,
        terminal: bool,
        emit_statement_nop: bool,
        exclude_terminal_body_not_taken: bool,
    ) -> Result<(), CompileError> {
        if statement.is_star {
            return self.compile_try_star_except(statement, terminal, emit_statement_nop);
        }
        if statement.handlers.is_empty() {
            return Err(unsupported("try statement without handlers"));
        }

        let base_depth = self.control.depth;
        let try_start = self.assembler.label();
        let try_end = self.assembler.label();
        let handler_start = self.assembler.label();
        let common_cleanup = self.assembler.label();
        let end = self.assembler.label();
        let body_ends_with_break_loop = statement.orelse.is_empty()
            && matches!(
                statement.body.last(),
                Some(Stmt::For(statement))
                    if statement.orelse.is_empty()
                        && suite_contains_loop_break(&statement.body)
            );

        if emit_statement_nop {
            let statement_nop_start = self.assembler.label();
            self.assembler.mark(statement_nop_start);
            self.assembler
                .set_location(self.source_location_including_trailing_semicolon(statement.range));
            self.emit(NOP, 0, 0)?;
            let statement_nop_end = self.assembler.label();
            self.assembler.mark(statement_nop_end);
            for exclusions in &mut self.control.active_exception_region_exclusions {
                exclusions.push((statement_nop_start, statement_nop_end));
            }
            for exclusions in &mut self.control.active_with_region_exclusions {
                exclusions.push((statement_nop_start, statement_nop_end));
            }
            if self.control.generator_region_start.is_some() {
                self.control
                    .generator_region_exclusions
                    .push((statement_nop_start, statement_nop_end));
            }
        }
        self.assembler.mark(try_start);
        self.control
            .active_exception_region_exclusions
            .push(Vec::new());
        let direct_continue =
            statement.orelse.is_empty() && matches!(statement.body.as_slice(), [Stmt::Continue(_)]);
        let body_noop_location = if direct_continue {
            self.assembler.mark(try_end);
            self.compile_statement(&statement.body[0])?;
            None
        } else {
            let body_instruction_count = self.assembler.instruction_count();
            if statement.body.len() > 1
                && let Some(Stmt::Pass(pass)) = statement.body.first()
            {
                self.assembler
                    .set_location(self.source_location(pass.range));
                self.emit(NOP, 0, 0)?;
            }
            if exclude_terminal_body_not_taken
                && let Some((last @ Stmt::If(_), leading)) = statement.body.split_last()
            {
                self.compile_suite(leading)?;
                self.with_control_override(
                    |control| &mut control.exclude_terminal_if_not_taken,
                    true,
                    |compiler| {
                        compiler.with_control_override(
                            |control| &mut control.exclude_condition_not_taken_from_exception,
                            true,
                            |compiler| compiler.compile_statement(last),
                        )
                    },
                )?;
            } else {
                self.compile_suite(&statement.body)?;
            }
            let location = (self.assembler.instruction_count() == body_instruction_count)
                .then(|| {
                    statement
                        .body
                        .last()
                        .map(|statement| self.source_location(statement.range()))
                })
                .flatten();
            self.assembler.mark(try_end);
            if !statement.orelse.is_empty()
                && let Some(location) = location
            {
                // If `try` and its pass body share a line, CPython retains the enclosing
                // statement location on the synthetic body marker.
                let location = self
                    .assembler
                    .last_instruction_location()
                    .filter(|previous| previous.line == location.line)
                    .unwrap_or(location);
                self.assembler.set_location(location);
                self.emit(NOP, 0, 0)?;
                // The no-op body does not inherit an enclosing finally's exception owner.
                if self.control.prevent_try_exit_inlining
                    && self.control.active_exception_region_exclusions.len() > 1
                {
                    self.assembler.exclude_last_instruction_from_exception();
                }
            }
            location
        };
        let try_exclusions = self
            .control
            .active_exception_region_exclusions
            .pop()
            .expect("active try region has exclusion collector");
        let orelse_instruction_count = self.assembler.instruction_count();
        self.compile_suite(&statement.orelse)?;
        let orelse_noop_location = (self.assembler.instruction_count() == orelse_instruction_count)
            .then(|| {
                statement
                    .orelse
                    .last()
                    .map(|statement| self.source_location(statement.range()))
            })
            .flatten();
        if direct_continue {
        } else if terminal {
            if let Some(location) = orelse_noop_location {
                self.assembler.set_location(location);
            } else if matches!(statement.body.last(), Some(Stmt::Try(_))) {
                self.assembler.set_location(SourceLocation::NONE);
            } else if statement.orelse.is_empty()
                && let Some(location) = body_noop_location
            {
                self.assembler.set_location(location);
            } else if let Some(location) = self.assembler.last_instruction_location() {
                self.assembler.set_location(location);
            }
            self.emit_deferred_implicit_return()?;
        } else {
            if let Some(location) = orelse_noop_location {
                self.assembler.set_location(location);
            } else if statement.orelse.is_empty()
                && let Some(Stmt::If(statement)) = statement.body.last()
                && let Some(test) = terminating_if_fallthrough_test(statement)
            {
                self.assembler
                    .set_location(self.source_location(test.range()));
            } else if statement.orelse.is_empty()
                && let Some(location) = body_noop_location
            {
                self.assembler.set_location(location);
            } else if let Some(location) = self.assembler.last_instruction_location() {
                self.assembler.set_location(location);
            }
            let pass_only_handlers = statement.handlers.iter().all(|handler| {
                handler
                    .as_except_handler()
                    .is_some_and(|handler| matches!(handler.body.as_slice(), [Stmt::Pass(_)]))
            });
            let body_ends_with_overridden_return = self.control.active_overriding_finally_returns
                > 0
                && matches!(statement.body.last(), Some(Stmt::Return(_)));
            let exit_exclusion_start =
                if !self.control.active_exception_region_exclusions.is_empty()
                    && (body_ends_with_overridden_return
                        || (body_noop_location.is_some()
                            && (self.control.active_overriding_finally_returns > 0
                                || (pass_only_handlers
                                    && !self.control.prevent_try_exit_inlining))))
                {
                    let start = self.assembler.label();
                    self.assembler.mark(start);
                    Some(start)
                } else {
                    None
                };
            self.emit_jump_forward(JUMP_FORWARD, end, 0)?;
            // A pass-only nested try leaves an ownerless exit NOP unless the enclosing region
            // is the synthetic protected body of try/finally.
            if body_noop_location.is_some()
                && statement.orelse.is_empty()
                && !self.control.active_exception_region_exclusions.is_empty()
                && self.control.active_finally_try_bodies == 0
            {
                self.assembler.exclude_last_instruction_from_exception();
            }
            if body_ends_with_break_loop {
                // CPython's `POP_BLOCK` initially prevents the loop break from threading
                // through this normal try exit. Once `POP_BLOCK` disappears, the exit jump
                // is retained as the loop cleanup's source-position NOP.
                self.assembler
                    .preserve_last_redundant_jump_nop_after_threading();
            }
            if statement.orelse.is_empty() && body_noop_location.is_some() {
                // Keep the pass line when direct exit duplication replaces this jump.
                self.assembler.preserve_last_direct_inlined_jump_nop();
            }
            if let Some(exit_exclusion_start) = exit_exclusion_start {
                let exit_exclusion_end = self.assembler.label();
                self.assembler.mark(exit_exclusion_end);
                for exclusions in &mut self.control.active_exception_region_exclusions {
                    exclusions.push((exit_exclusion_start, exit_exclusion_end));
                }
            }
            if self.control.prevent_try_exit_inlining {
                self.assembler.prevent_last_jump_inlining();
            }
        }
        self.add_exception_regions_with_exclusions(
            try_start,
            try_end,
            &try_exclusions,
            handler_start,
            base_depth.cast_unsigned(),
            false,
        );

        self.assembler.mark(handler_start);
        self.set_depth(base_depth + 1);
        self.assembler.set_location(SourceLocation::NONE);
        self.emit(PUSH_EXC_INFO, 0, 1)?;
        let mut dispatch_start = handler_start;
        let mut final_handler_location = SourceLocation::NONE;

        for (handler_index, handler) in statement.handlers.iter().enumerate() {
            let handler = handler.as_except_handler().ok_or_else(|| {
                CompileError::Internal("invalid exception handler node".to_string())
            })?;
            let next_handler = self.assembler.label();
            let handler_location = self.source_location_including_trailing_semicolon(handler.range);
            final_handler_location = handler_location;
            self.set_depth(base_depth + 2);
            let mut not_taken_exclusion = None;
            if let Some(exception_type) = &handler.type_ {
                let mut references = ReferenceCollector::default();
                references.visit_expr(exception_type);
                let reference_names = references
                    .references
                    .into_iter()
                    .map(|name| self.required_name_id(&name))
                    .collect::<Vec<_>>();
                let newly_owned: Vec<_> = reference_names
                    .into_iter()
                    .filter(|name| self.symbols.owned_load_locals.insert(*name))
                    .collect();
                self.compile_expression(exception_type)?;
                for name in newly_owned {
                    self.symbols.owned_load_locals.remove(name);
                }
                self.assembler.set_location(handler_location);
                self.emit(CHECK_EXC_MATCH, 0, 0)?;
                self.emit_jump_forward(POP_JUMP_IF_FALSE, next_handler, -1)?;
                let exclusion_start = self.assembler.label();
                self.assembler.mark(exclusion_start);
                self.emit(NOT_TAKEN, 0, 0)?;
                let exclusion_end = self.assembler.label();
                self.assembler.mark(exclusion_end);
                if handler_index > 0
                    || matches!(exception_type.as_ref(), Expr::BoolOp(_) | Expr::If(_))
                {
                    for exclusions in &mut self.control.active_exception_region_exclusions {
                        exclusions.push((exclusion_start, exclusion_end));
                    }
                    not_taken_exclusion = Some((exclusion_start, exclusion_end));
                }
            } else if handler_index + 1 != statement.handlers.len() {
                return Err(CompileError::Internal(
                    "bare except handler must be last".to_string(),
                ));
            }

            self.assembler.set_location(handler_location);
            if let Some(name) = &handler.name {
                self.store_name(name.as_str())?;
            } else {
                self.emit(POP_TOP, 0, -1)?;
            }

            let body_start = self.assembler.label();
            let body_end = self.assembler.label();
            let name_cleanup = self.assembler.label();
            self.assembler.mark(body_start);
            if matches!(handler.body.as_slice(), [Stmt::Continue(_)]) {
                self.assembler.mark(body_end);
                self.assembler.add_exception_region(
                    dispatch_start,
                    body_end,
                    common_cleanup,
                    (base_depth + 1).cast_unsigned(),
                    true,
                );
                self.assembler
                    .set_location(self.source_location(handler.body[0].range()));
                self.emit(POP_EXCEPT, 0, -1)?;
                if let Some(name) = &handler.name {
                    let none = self.add_constant(Constant::None)?;
                    self.emit(LOAD_CONST, none, 1)?;
                    self.store_name(name.as_str())?;
                    self.delete_name(name.as_str())?;
                }
                let continue_label = self
                    .control
                    .loops
                    .last()
                    .map(|context| context.continue_label)
                    .ok_or_else(|| CompileError::Internal("continue outside loop".to_string()))?;
                self.emit_jump_backward(JUMP_BACKWARD, continue_label, 0)?;
                if let Some(name) = &handler.name {
                    self.assembler.add_exception_region(
                        body_start,
                        body_end,
                        name_cleanup,
                        (base_depth + 1).cast_unsigned(),
                        true,
                    );
                    self.assembler.mark(name_cleanup);
                    self.set_depth(base_depth + 3);
                    self.assembler.set_location(SourceLocation::NONE);
                    let none = self.add_constant(Constant::None)?;
                    self.emit(LOAD_CONST, none, 1)?;
                    self.store_name(name.as_str())?;
                    self.delete_name(name.as_str())?;
                    self.emit(RERAISE, 1, -3)?;
                }
                self.assembler.mark(next_handler);
                dispatch_start = if handler.name.is_some() {
                    name_cleanup
                } else {
                    next_handler
                };
                continue;
            }
            if handler.name.is_none() && matches!(handler.body.as_slice(), [Stmt::Break(_)]) {
                self.assembler.mark(body_end);
                self.assembler.add_exception_region(
                    dispatch_start,
                    body_end,
                    common_cleanup,
                    (base_depth + 1).cast_unsigned(),
                    true,
                );
                self.assembler
                    .set_location(self.source_location(handler.body[0].range()));
                self.emit(POP_EXCEPT, 0, -1)?;
                let unreachable_depth = self.control.depth;
                let context = self
                    .control
                    .loops
                    .last()
                    .copied()
                    .ok_or_else(|| CompileError::Internal("break outside loop".to_string()))?;
                match context.iterator_cleanup {
                    IteratorCleanup::None => {}
                    IteratorCleanup::Sync | IteratorCleanup::Async => {
                        self.emit(POP_TOP, 0, -1)?;
                    }
                }
                if context.break_returns {
                    self.emit_implicit_return()?;
                } else {
                    self.emit_jump_forward(JUMP_BACKWARD, context.break_label, 0)?;
                }
                self.set_depth(unreachable_depth);
                self.assembler.mark(next_handler);
                dispatch_start = next_handler;
                continue;
            }
            if handler.name.is_none()
                && !self.control.active_pass_finally_locations.is_empty()
                && let [Stmt::Return(return_statement)] = handler.body.as_slice()
                && return_statement
                    .value
                    .as_deref()
                    .is_none_or(|value| matches!(value, Expr::NoneLiteral(_)))
            {
                self.assembler.mark(body_end);
                self.assembler.add_exception_region(
                    dispatch_start,
                    body_end,
                    common_cleanup,
                    (base_depth + 1).cast_unsigned(),
                    true,
                );
                let return_range = return_statement
                    .value
                    .as_deref()
                    .map_or(return_statement.range, Ranged::range);
                self.assembler
                    .set_location(self.source_location(return_range));
                self.emit(POP_EXCEPT, 0, -1)?;
                let unwind_start = self.assembler.label();
                self.assembler.mark(unwind_start);
                self.emit_pass_finally_nops()?;
                self.assembler.set_location(SourceLocation::NONE);
                let none = self.add_constant(Constant::None)?;
                self.emit(LOAD_CONST, none, 1)?;
                self.emit(RETURN_VALUE, 0, -1)?;
                let unwind_end = self.assembler.label();
                self.assembler.mark(unwind_end);
                for exclusions in &mut self.control.active_exception_region_exclusions {
                    exclusions.push((unwind_start, unwind_end));
                }
                self.assembler.mark(next_handler);
                dispatch_start = next_handler;
                continue;
            }
            let handler_body_start = self.assembler.instruction_count();
            let newly_owned_handler_locals = {
                let initialized_locals = &self.symbols.initialized_locals;
                let owned_load_locals = &mut self.symbols.owned_load_locals;
                initialized_locals
                    .iter_unordered()
                    .filter(|name| owned_load_locals.insert(*name))
                    .collect::<Vec<_>>()
            };
            let handler_name = handler
                .name
                .as_ref()
                .map(|name| self.required_name_id(name.as_str()));
            let previously_owned =
                handler_name.is_some_and(|name| !self.symbols.owned_load_locals.insert(name));
            let terminal_handler_if = if terminal
                && let Some(name) = &handler.name
                && let [Stmt::If(statement)] = handler.body.as_slice()
                && terminal_exception_handler_if_supported(statement)
            {
                Some((statement, name.as_str()))
            } else {
                None
            };
            // CPython terminal-compiles a final nested try while the outer handler fblock is
            // still active, so each implicit return carries the outer handler cleanup.
            let terminal_handler_try = terminal
                && terminal_handler_if.is_none()
                && matches!(handler.body.last(), Some(Stmt::Try(_)));
            let mut terminal_handler_try_return = false;
            let (terminal_handler_exclusions, mut handler_region_exclusions) =
                if let Some((statement, name)) = terminal_handler_if {
                    (
                        Some(self.compile_terminal_exception_handler_if(
                            statement,
                            name,
                            base_depth + 1,
                        )?),
                        Vec::new(),
                    )
                } else {
                    let strict_owned_loads = matches!(
                        handler.body.last(),
                        Some(Stmt::Break(_) | Stmt::Continue(_))
                    );
                    let context = ExceptionHandlerContext {
                        name: handler.name.as_ref().map(ToString::to_string),
                        loop_depth: self.control.loops.len(),
                    };
                    let ((), exclusions) = self.with_exception_handler(context, |compiler| {
                        compiler.with_control_override(
                            |control| &mut control.unwind_exception_handlers_for_implicit_return,
                            terminal_handler_try,
                            |compiler| {
                                if terminal_handler_try {
                                    let ((), emitted_fallthrough) = compiler
                                        .with_fallthrough_tracking(|compiler| {
                                            compiler.compile_with_strict_owned_loads(
                                                strict_owned_loads,
                                                |compiler| {
                                                    compiler
                                                        .compile_suite_inner(&handler.body, true)
                                                },
                                            )
                                        })?;
                                    terminal_handler_try_return = emitted_fallthrough;
                                    Ok(())
                                } else {
                                    compiler.compile_with_strict_owned_loads(
                                        strict_owned_loads,
                                        |compiler| compiler.compile_suite(&handler.body),
                                    )
                                }
                            },
                        )
                    })?;
                    (None, exclusions)
                };
            if let Some(exclusions) = &terminal_handler_exclusions {
                handler_region_exclusions.extend_from_slice(exclusions);
            }
            if let Some(name) = handler_name
                && !previously_owned
            {
                self.symbols.owned_load_locals.remove(name);
            }
            for name in newly_owned_handler_locals {
                self.symbols.owned_load_locals.remove(name);
            }
            let handler_body_has_instructions =
                self.assembler.instruction_count() > handler_body_start;
            self.assembler.mark(body_end);
            if handler.name.is_some() {
                self.add_exception_regions_with_exclusions(
                    body_start,
                    body_end,
                    &handler_region_exclusions,
                    name_cleanup,
                    (base_depth + 1).cast_unsigned(),
                    true,
                );
            }
            let mut dispatch_exclusions = Vec::new();
            if let Some(exclusion) = not_taken_exclusion {
                dispatch_exclusions.push(exclusion);
            }
            dispatch_exclusions.extend_from_slice(&handler_region_exclusions);
            self.add_exception_regions_with_exclusions(
                dispatch_start,
                body_end,
                &dispatch_exclusions,
                common_cleanup,
                (base_depth + 1).cast_unsigned(),
                true,
            );
            if terminal_handler_exclusions.is_none() && !terminal_handler_try_return {
                if !handler_body_has_instructions && let Some(statement) = handler.body.last() {
                    self.assembler
                        .set_location(self.source_location(statement.range()));
                // CPython leaves named cleanup after a nested try unlocated so every incoming
                // edge can inline its own copy with the predecessor's line.
                } else if handler.name.is_some()
                    && matches!(handler.body.last(), Some(Stmt::Try(_)))
                {
                    self.assembler.set_location(SourceLocation::NONE);
                } else if let Some(location) = self.assembler.last_instruction_location() {
                    self.assembler.set_location(location);
                }
                self.emit(POP_EXCEPT, 0, -1)?;
                if let Some(name) = &handler.name {
                    let none = self.add_constant(Constant::None)?;
                    self.emit(LOAD_CONST, none, 1)?;
                    self.store_name(name.as_str())?;
                    self.delete_name(name.as_str())?;
                }
                if terminal {
                    self.emit_deferred_implicit_return()?;
                } else {
                    self.emit_jump_forward(JUMP_FORWARD, end, 0)?;
                    if handler.name.is_some() && matches!(handler.body.last(), Some(Stmt::Try(_))) {
                        // The cleanup is larger than a small exit, but is still eligible because
                        // it has no line number and cannot fall through.
                        self.assembler.prepare_last_no_location_block_for_inlining();
                    }
                    // A pass-only finally keeps these extended handler exits in its protected
                    // region; executable finally bodies use CPython's stale-owner exclusion.
                    if !handler_body_has_instructions
                        && !self.control.active_exception_region_exclusions.is_empty()
                        && self.control.active_pass_finally_locations.is_empty()
                    {
                        self.assembler
                            .exclude_last_instruction_from_exception_if_extended();
                    }
                    let return_is_overridden = self.control.active_overriding_finally_returns > 0
                        && matches!(handler.body.last(), Some(Stmt::Return(_)));
                    if self.control.prevent_try_exit_inlining && !return_is_overridden {
                        self.assembler.prevent_last_jump_inlining();
                    }
                }
            }

            if let Some(name) = &handler.name {
                self.assembler.mark(name_cleanup);
                self.set_depth(base_depth + 3);
                self.assembler.set_location(SourceLocation::NONE);
                let none = self.add_constant(Constant::None)?;
                self.emit(LOAD_CONST, none, 1)?;
                self.store_name(name.as_str())?;
                self.delete_name(name.as_str())?;
                self.emit(RERAISE, 1, -3)?;
            }

            self.assembler.mark(next_handler);
            dispatch_start = if handler.name.is_some() {
                name_cleanup
            } else {
                next_handler
            };
        }

        if statement.handlers.last().is_some_and(|handler| {
            handler
                .as_except_handler()
                .is_some_and(|handler| handler.type_.is_some())
        }) {
            self.set_depth(base_depth + 2);
            self.assembler.set_location(final_handler_location);
            self.emit(RERAISE, 0, -2)?;
            let dispatch_end = self.assembler.label();
            self.assembler.mark(dispatch_end);
            self.assembler.add_exception_region(
                dispatch_start,
                dispatch_end,
                common_cleanup,
                (base_depth + 1).cast_unsigned(),
                true,
            );
        }

        self.assembler.mark(common_cleanup);
        self.set_depth(base_depth + 3);
        self.assembler.set_location(SourceLocation::NONE);
        self.emit(COPY, 3, 1)?;
        self.emit(POP_EXCEPT, 0, -1)?;
        self.emit(RERAISE, 1, -3)?;

        if statement.handlers.iter().all(|handler| {
            handler.as_except_handler().is_some_and(|handler| {
                matches!(
                    handler.body.last(),
                    Some(Stmt::Break(_) | Stmt::Continue(_))
                )
            })
        }) {
            self.assembler.prevent_borrow_reachability(end);
        }
        self.assembler.mark(end);
        self.set_depth(base_depth);
        Ok(())
    }

    fn compile_try_star_except(
        &mut self,
        statement: &ruff_python_ast::StmtTry,
        terminal: bool,
        emit_statement_nop: bool,
    ) -> Result<(), CompileError> {
        let base_depth = self.control.depth;
        let try_start = self.assembler.label();
        let try_end = self.assembler.label();
        let handler_start = self.assembler.label();
        let orelse = self.assembler.label();
        let end = self.assembler.label();
        let common_cleanup = self.assembler.label();
        let reraise_star = self.assembler.label();
        let reraise = self.assembler.label();

        if emit_statement_nop {
            let statement_nop_start = self.assembler.label();
            self.assembler.mark(statement_nop_start);
            self.assembler
                .set_location(self.source_location_including_trailing_semicolon(statement.range));
            self.emit(NOP, 0, 0)?;
            let statement_nop_end = self.assembler.label();
            self.assembler.mark(statement_nop_end);
            for exclusions in &mut self.control.active_exception_region_exclusions {
                exclusions.push((statement_nop_start, statement_nop_end));
            }
        }
        self.assembler.mark(try_start);
        self.control
            .active_exception_region_exclusions
            .push(Vec::new());
        let body_instruction_start = self.assembler.instruction_count();
        self.compile_suite(&statement.body)?;
        let body_noop_location = (self.assembler.instruction_count() == body_instruction_start)
            .then(|| {
                statement
                    .body
                    .last()
                    .map(|statement| self.source_location(statement.range()))
            })
            .flatten();
        let body_fallthrough_location =
            body_noop_location.or_else(|| self.assembler.last_instruction_location());
        let exit_exclusion_start = if body_noop_location.is_some()
            && statement.orelse.is_empty()
            && !self.control.prevent_try_exit_inlining
            && !self.control.active_exception_region_exclusions.is_empty()
        {
            let start = self.assembler.label();
            self.assembler.mark(start);
            Some(start)
        } else {
            None
        };
        if !terminal && let Some(location) = body_noop_location {
            self.assembler.set_location(location);
            self.emit(NOP, 0, 0)?;
        }
        self.assembler.mark(try_end);
        let try_exclusions = self
            .control
            .active_exception_region_exclusions
            .pop()
            .expect("active try-star region has exclusion collector");
        self.assembler.set_location(if terminal {
            body_fallthrough_location.unwrap_or(SourceLocation::NONE)
        } else {
            SourceLocation::NONE
        });
        self.emit_jump_forward(JUMP_FORWARD, orelse, 0)?;
        if let Some(exit_exclusion_start) = exit_exclusion_start {
            let exit_exclusion_end = self.assembler.label();
            self.assembler.mark(exit_exclusion_end);
            for exclusions in &mut self.control.active_exception_region_exclusions {
                exclusions.push((exit_exclusion_start, exit_exclusion_end));
            }
        }
        self.add_exception_regions_with_exclusions(
            try_start,
            try_end,
            &try_exclusions,
            handler_start,
            base_depth.cast_unsigned(),
            false,
        );

        self.assembler.mark(handler_start);
        self.set_depth(base_depth + 1);
        self.assembler.set_location(SourceLocation::NONE);
        self.emit(PUSH_EXC_INFO, 0, 1)?;
        let handler_region_start = handler_start;
        let mut handler_region_exclusions = Vec::new();

        for (handler_index, handler) in statement.handlers.iter().enumerate() {
            let handler = handler.as_except_handler().ok_or_else(|| {
                CompileError::Internal("invalid exception-group handler node".to_string())
            })?;
            let handler_location = self.source_location_including_trailing_semicolon(handler.range);
            let next_handler = self.assembler.label();
            let handler_error = self.assembler.label();
            let no_match = self.assembler.label();
            let body_cleanup = self.assembler.label();

            self.set_depth(base_depth + 2);
            if handler_index == 0 {
                self.assembler.set_location(handler_location);
                self.emit(BUILD_LIST, 0, 1)?;
                self.emit(COPY, 2, 1)?;
            }
            let exception_type = handler.type_.as_ref().ok_or_else(|| {
                CompileError::Internal("exception-group handler has no type".to_string())
            })?;
            self.compile_expression(exception_type)?;
            self.assembler.set_location(handler_location);
            self.emit(CHECK_EG_MATCH, 0, 0)?;
            self.emit(COPY, 1, 1)?;
            self.emit_jump_forward(POP_JUMP_IF_NONE, no_match, -1)?;
            let not_taken_exclusion_start =
                (handler_index > 0 || matches!(exception_type.as_ref(), Expr::If(_))).then(|| {
                    let start = self.assembler.label();
                    self.assembler.mark(start);
                    start
                });
            self.emit(NOT_TAKEN, 0, 0)?;
            if let Some(start) = not_taken_exclusion_start {
                let end = self.assembler.label();
                self.assembler.mark(end);
                handler_region_exclusions.push((start, end));
                // The synthetic second-handler marker belongs to neither the group handler nor
                // an enclosing finally region.
                for exclusions in &mut self.control.active_exception_region_exclusions {
                    exclusions.push((start, end));
                }
            }

            if let Some(name) = &handler.name {
                self.store_name(name.as_str())?;
            } else {
                self.emit(POP_TOP, 0, -1)?;
            }
            let body_start = self.assembler.label();
            self.assembler.mark(body_start);
            let body_instruction_start = self.assembler.instruction_count();
            self.compile_suite(&handler.body)?;
            let body_has_instructions = self.assembler.instruction_count() > body_instruction_start;
            let body_end = self.assembler.label();
            self.assembler.mark(body_end);
            if let Some(name) = &handler.name {
                if body_has_instructions
                    && let Some(location) = self.assembler.last_instruction_location()
                {
                    self.assembler.set_location(location);
                } else if let Some(statement) = handler.body.last() {
                    self.assembler
                        .set_location(self.source_location(statement.range()));
                }
                let none = self.add_constant(Constant::None)?;
                self.emit(LOAD_CONST, none, 1)?;
                self.store_name(name.as_str())?;
                self.delete_name(name.as_str())?;
            }
            let is_last = handler_index + 1 == statement.handlers.len();
            if is_last {
                if body_has_instructions
                    && let Some(location) = self.assembler.last_instruction_location()
                {
                    self.assembler.set_location(location);
                } else if let Some(statement) = handler.body.last() {
                    self.assembler
                        .set_location(self.source_location(statement.range()));
                }
                self.emit(LIST_APPEND, 1, -1)?;
                self.emit_jump_forward(JUMP_FORWARD, reraise_star, 0)?;
            } else {
                if body_has_instructions
                    && let Some(location) = self.assembler.last_instruction_location()
                {
                    self.assembler.set_location(location);
                } else if let Some(statement) = handler.body.last() {
                    self.assembler
                        .set_location(self.source_location(statement.range()));
                }
                self.emit_jump_forward(JUMP_FORWARD, next_handler, 0)?;
            }

            self.assembler.add_exception_region(
                body_start,
                body_end,
                body_cleanup,
                (base_depth + 4).cast_unsigned(),
                true,
            );
            self.assembler.mark(body_cleanup);
            self.set_depth(base_depth + 6);
            self.assembler.set_location(SourceLocation::NONE);
            if let Some(name) = &handler.name {
                let none = self.add_constant(Constant::None)?;
                self.emit(LOAD_CONST, none, 1)?;
                self.store_name(name.as_str())?;
                self.delete_name(name.as_str())?;
            }
            self.emit(LIST_APPEND, 3, -1)?;
            self.emit(POP_TOP, 0, -1)?;
            if is_last {
                self.emit(LIST_APPEND, 1, -1)?;
                self.emit_jump_forward(JUMP_FORWARD, reraise_star, 0)?;
            } else {
                self.emit_jump_forward(JUMP_FORWARD, handler_error, 0)?;
            }

            if !is_last {
                self.assembler.mark(next_handler);
                self.set_depth(base_depth + 4);
                self.emit(NOP, 0, 0)?;
                self.emit_jump_forward(JUMP_FORWARD, handler_error, 0)?;
            }

            self.assembler.mark(no_match);
            self.set_depth(base_depth + 5);
            self.assembler.set_location(handler_location);
            self.emit(POP_TOP, 0, -1)?;

            self.set_depth(base_depth + 4);
            if is_last {
                self.assembler.set_location(SourceLocation::NONE);
                self.emit(LIST_APPEND, 1, -1)?;
                self.emit_jump_forward(JUMP_FORWARD, reraise_star, 0)?;
            } else {
                self.assembler.mark(handler_error);
            }
        }

        self.assembler.mark(reraise_star);
        self.set_depth(base_depth + 3);
        self.assembler.set_location(SourceLocation::NONE);
        self.emit_binary_intrinsic(BinaryIntrinsic::PREP_RERAISE_STAR)?;
        self.emit(COPY, 1, 1)?;
        self.emit_jump_forward(POP_JUMP_IF_NOT_NONE, reraise, -1)?;
        let exclusion_start = self.assembler.label();
        self.assembler.mark(exclusion_start);
        self.emit(NOT_TAKEN, 0, 0)?;
        let exclusion_end = self.assembler.label();
        self.assembler.mark(exclusion_end);
        handler_region_exclusions.push((exclusion_start, exclusion_end));
        for exclusions in &mut self.control.active_exception_region_exclusions {
            exclusions.push((exclusion_start, exclusion_end));
        }
        self.emit(POP_TOP, 0, -1)?;
        let handler_region_end = self.assembler.label();
        self.assembler.mark(handler_region_end);
        self.emit(POP_EXCEPT, 0, -1)?;
        self.emit_jump_forward(JUMP_FORWARD, end, 0)?;

        self.assembler.mark(reraise);
        self.set_depth(base_depth + 2);
        self.emit(SWAP, 2, 0)?;
        self.emit(POP_EXCEPT, 0, -1)?;
        self.emit(RERAISE, 0, -1)?;
        self.add_exception_regions_with_exclusions(
            handler_region_start,
            handler_region_end,
            &handler_region_exclusions,
            common_cleanup,
            (base_depth + 1).cast_unsigned(),
            true,
        );

        self.assembler.mark(common_cleanup);
        self.set_depth(base_depth + 3);
        self.assembler.set_location(SourceLocation::NONE);
        self.emit(COPY, 3, 1)?;
        self.emit(POP_EXCEPT, 0, -1)?;
        self.emit(RERAISE, 1, -3)?;

        self.assembler.mark(orelse);
        self.set_depth(base_depth);
        let orelse_instruction_start = self.assembler.instruction_count();
        self.compile_suite(&statement.orelse)?;
        // A protected try-star else suite retains its last pass marker.
        if self.assembler.instruction_count() == orelse_instruction_start
            && let Some(Stmt::Pass(statement)) = statement.orelse.last()
        {
            self.assembler
                .set_location(self.source_location(statement.range));
            self.emit(NOP, 0, 0)?;
        }
        self.assembler.mark(end);
        self.set_depth(base_depth);
        if terminal {
            if let Some(location) = self.assembler.last_instruction_location() {
                self.assembler.set_location(location);
            }
            self.emit_implicit_return()?;
        }
        Ok(())
    }

    fn compile_terminal_exception_handler_if(
        &mut self,
        statement: &ruff_python_ast::StmtIf,
        name: &str,
        handler_depth: i32,
    ) -> Result<Vec<(Label, Label)>, CompileError> {
        let mut branches: Vec<(Option<&Expr>, &[Stmt])> =
            vec![(Some(&statement.test), statement.body.as_slice())];
        branches.extend(
            statement
                .elif_else_clauses
                .iter()
                .map(|clause| (clause.test.as_ref(), clause.body.as_slice())),
        );
        let has_else = branches.last().is_some_and(|(test, _)| test.is_none());
        let branch_count = branches.len();
        let mut exclusions = Vec::new();
        let mut last_test: &Expr = statement.test.as_ref();

        for (branch_index, (test, body)) in branches.into_iter().enumerate() {
            let next = if let Some(test) = test {
                last_test = test;
                let next = self.assembler.label();
                if branch_index > 0 && branch_index + 1 == branch_count && !has_else {
                    let ((), branch_exclusions) = self.with_region_exclusion_collector(
                        |control| &mut control.active_with_region_exclusions,
                        |compiler| {
                            compiler.with_control_override(
                                |control| &mut control.exclude_terminal_if_not_taken,
                                true,
                                |compiler| compiler.compile_jump_if(test, false, next),
                            )
                        },
                    )?;
                    exclusions.extend(branch_exclusions);
                } else {
                    self.compile_jump_if(test, false, next)?;
                }
                Some(next)
            } else {
                None
            };

            let action_location = if matches!(body, [Stmt::Return(statement)] if statement.value.is_none())
            {
                self.source_location(body[0].range())
            } else {
                let instruction_count = self.assembler.instruction_count();
                self.compile_suite(body)?;
                if self.assembler.instruction_count() > instruction_count {
                    self.assembler
                        .last_instruction_location()
                        .unwrap_or_else(|| self.source_location(last_test.range()))
                } else {
                    body.last().map_or_else(
                        || self.source_location(last_test.range()),
                        |statement| self.source_location(statement.range()),
                    )
                }
            };
            exclusions.push(self.emit_terminal_exception_handler_action(
                name,
                handler_depth,
                action_location,
            )?);

            if let Some(next) = next {
                self.assembler.mark(next);
                self.set_depth(handler_depth);
            }
        }
        if !has_else {
            exclusions.push(self.emit_terminal_exception_handler_action(
                name,
                handler_depth,
                self.source_location(last_test.range()),
            )?);
        }
        Ok(exclusions)
    }

    fn emit_terminal_exception_handler_action(
        &mut self,
        name: &str,
        handler_depth: i32,
        location: SourceLocation,
    ) -> Result<(Label, Label), CompileError> {
        let start = self.assembler.label();
        self.assembler.mark(start);
        self.assembler.set_location(location);
        self.emit(POP_EXCEPT, 0, -1)?;
        let none = self.add_constant(Constant::None)?;
        self.emit(LOAD_CONST, none, 1)?;
        self.store_name(name)?;
        self.delete_name(name)?;
        self.emit_implicit_return()?;
        let end = self.assembler.label();
        self.assembler.mark(end);
        self.set_depth(handler_depth);
        Ok((start, end))
    }

    fn add_exception_regions_with_exclusions(
        &mut self,
        start: Label,
        end: Label,
        exclusions: &[(Label, Label)],
        target: Label,
        depth: u32,
        preserve_lasti: bool,
    ) {
        let mut region_start = start;
        for (exclusion_start, exclusion_end) in exclusions {
            self.assembler.add_exception_region(
                region_start,
                *exclusion_start,
                target,
                depth,
                preserve_lasti,
            );
            region_start = *exclusion_end;
        }
        self.assembler
            .add_exception_region(region_start, end, target, depth, preserve_lasti);
    }

    fn compile_try_finally(
        &mut self,
        statement: &ruff_python_ast::StmtTry,
        terminal: bool,
    ) -> Result<(), CompileError> {
        if statement.handlers.is_empty()
            && statement.orelse.is_empty()
            && matches!(statement.body.as_slice(), [Stmt::Continue(_)])
            && matches!(statement.finalbody.as_slice(), [Stmt::Break(_)])
        {
            return self.compile_try_finally_continue_break(statement);
        }
        let base_depth = self.control.depth;
        let protected_start = self.assembler.label();
        let protected_end = self.assembler.label();
        let finally_handler = self.assembler.label();
        let cleanup = self.assembler.label();
        let end = self.assembler.label();
        if !statement.handlers.is_empty() {
            // Removing the inner try's normal exit leaves an empty CPython CFG block before the
            // normal finally body. Its borrow traversal stops at that empty block.
            self.assembler.prevent_borrow_reachability(protected_end);
            if matches!(statement.finalbody.first(), Some(Stmt::Try(_))) {
                // Keep handler exits targeting the nested finally body instead of the removed
                // normal-exit NOP that precedes it.
                self.assembler.preserve_block_boundary(protected_end);
            }
        }

        let statement_nop_start = self.assembler.label();
        self.assembler.mark(statement_nop_start);
        self.emit(NOP, 0, 0)?;
        let statement_nop_end = self.assembler.label();
        self.assembler.mark(statement_nop_end);
        for exclusions in &mut self.control.active_exception_region_exclusions {
            exclusions.push((statement_nop_start, statement_nop_end));
        }
        for exclusions in &mut self.control.active_with_region_exclusions {
            exclusions.push((statement_nop_start, statement_nop_end));
        }
        if self.control.generator_region_start.is_some() {
            self.control
                .generator_region_exclusions
                .push((statement_nop_start, statement_nop_end));
        }
        self.assembler.mark(protected_start);
        let pass_finally_location = match statement.finalbody.as_slice() {
            [Stmt::Pass(statement)] => Some(self.source_location(statement.range)),
            _ => None,
        };
        let overriding_return = matches!(statement.finalbody.last(), Some(Stmt::Return(_)));
        let preserve_break_exit_loop_range = if overriding_return {
            match statement.body.last() {
                Some(Stmt::While(loop_statement))
                    if loop_statement.orelse.is_empty()
                        && suite_contains_loop_break(&loop_statement.body) =>
                {
                    Some(loop_statement.range)
                }
                Some(Stmt::For(loop_statement))
                    if loop_statement.orelse.is_empty()
                        && suite_contains_loop_break(&loop_statement.body) =>
                {
                    Some(loop_statement.range)
                }
                _ => self.control.preserve_finally_break_exit_loop_range,
            }
        } else {
            self.control.preserve_finally_break_exit_loop_range
        };
        let copy_finally_on_return = pass_finally_location.is_none()
            && !suite_terminates(&statement.finalbody)
            && self.control.active_exception_handlers.is_empty()
            && self.control.active_return_finally_contexts.is_empty();
        let return_context = copy_finally_on_return.then(|| ReturnFinallyContext {
            body: statement.finalbody.to_vec(),
            loop_depth: self.control.loops.len(),
            handler: finally_handler,
            depth: base_depth.cast_unsigned(),
        });
        let (protected_has_instructions, protected_exclusions) = self
            .with_finally_protected_context(
                pass_finally_location,
                overriding_return,
                preserve_break_exit_loop_range,
                return_context,
                |compiler| {
                    if statement.handlers.is_empty() {
                        let instruction_count = compiler.assembler.instruction_count();
                        if statement.body.len() > 1
                            && let Some(Stmt::Pass(pass)) = statement.body.first()
                        {
                            compiler
                                .assembler
                                .set_location(compiler.source_location(pass.range));
                            compiler.emit(NOP, 0, 0)?;
                        }
                        compiler.compile_suite(&statement.body)?;
                        let has_instructions =
                            compiler.assembler.instruction_count() > instruction_count;
                        if !has_instructions {
                            if let Some(last) = statement.body.last() {
                                compiler
                                    .assembler
                                    .set_location(compiler.source_location(last.range()));
                            }
                            compiler.emit(NOP, 0, 0)?;
                            // A no-op protected body does not acquire an enclosing try's handler.
                            if compiler.control.active_exception_region_exclusions.len() > 1 {
                                compiler.assembler.exclude_last_instruction_from_exception();
                            }
                        }
                        Ok(has_instructions)
                    } else {
                        compiler.with_control_override(
                            |control| &mut control.prevent_try_exit_inlining,
                            true,
                            |compiler| compiler.compile_try_except(statement, false, false, false),
                        )?;
                        Ok(true)
                    }
                },
            )?;
        self.assembler.mark(protected_end);
        if terminal
            && pass_finally_location.is_some()
            && !statement.handlers.is_empty()
            && suite_terminates(&statement.body)
            && statement.handlers.iter().any(|handler| {
                handler
                    .as_except_handler()
                    .is_some_and(|handler| !suite_terminates(&handler.body))
            })
            && let Some(location) = statement.handlers.iter().rev().find_map(|handler| {
                handler
                    .as_except_handler()
                    .and_then(|handler| handler.body.last())
                    .map(|statement| self.source_location(statement.range()))
            })
        {
            let exit_nop_start = self.assembler.label();
            self.assembler.mark(exit_nop_start);
            self.assembler.set_location(location);
            self.emit(NOP, 0, 0)?;
            let exit_nop_end = self.assembler.label();
            self.assembler.mark(exit_nop_end);
            if self.control.generator_region_start.is_some() {
                self.control
                    .generator_region_exclusions
                    .push((exit_nop_start, exit_nop_end));
            }
        }
        // `POP_BLOCK` is converted to a NOP before CPython's CFG optimizer runs. It normally
        // disappears, but can retain the sole predecessor's line when a small finally exit is
        // copied onto another edge.
        self.assembler.set_location(SourceLocation::NONE);
        self.emit(NOP, 0, 0)?;
        let noop_finally_body = statement.finalbody.iter().all(|statement| {
            matches!(statement, Stmt::Pass(_))
                || matches!(statement, Stmt::Expr(expression) if fold_constant(&expression.value).is_some())
        });
        // A handler-form try leaves its converted POP_BLOCK unlocated when the normal finally
        // body has no executable instructions. Otherwise, predecessor propagation can retain it.
        if statement.handlers.is_empty() || !noop_finally_body {
            self.assembler.mark_last_as_converted_pop_block();
        } else {
            self.assembler.preserve_last_no_location();
        }
        let ((), finalbody_emitted_fallthrough) = self.with_fallthrough_tracking(|compiler| {
            if let Some(location) = pass_finally_location {
                compiler.assembler.set_location(location);
                if compiler
                    .control
                    .active_exception_region_exclusions
                    .is_empty()
                {
                    compiler.emit(NOP, 0, 0)?;
                } else {
                    // The normal finally copy is outside an enclosing try's exception ownership.
                    // Mark both the pass and its exit because CFG optimization can keep either NOP.
                    let pass_start = compiler.assembler.label();
                    compiler.assembler.mark(pass_start);
                    compiler.emit(NOP, 0, 0)?;
                    compiler.assembler.exclude_last_instruction_from_exception();
                    let pass_end = compiler.assembler.label();
                    compiler.assembler.mark(pass_end);
                    for exclusions in &mut compiler.control.active_exception_region_exclusions {
                        exclusions.push((pass_start, pass_end));
                    }
                }
            } else {
                compiler.with_control_counter(
                    |control| &mut control.active_normal_finally_bodies,
                    |compiler| compiler.compile_suite_inner(&statement.finalbody, terminal),
                )?;
            }
            Ok(())
        })?;
        if terminal && !finalbody_emitted_fallthrough && !suite_terminates(&statement.finalbody) {
            if let Some(statement) = statement
                .finalbody
                .last()
                .filter(|statement| statement_uses_implicit_return_location(statement))
            {
                self.assembler
                    .set_location(self.source_location(implicit_return_range(self, statement)));
            } else if let Some(location) = self.assembler.last_instruction_location() {
                self.assembler.set_location(location);
            }
            self.emit_implicit_return()?;
        } else if !terminal {
            if let Some(location) = self.assembler.last_instruction_location() {
                self.assembler.set_location(location);
            }
            self.emit_jump_forward(JUMP_FORWARD, end, 0)?;
            if pass_finally_location.is_some()
                && !self.control.active_exception_region_exclusions.is_empty()
            {
                self.assembler.exclude_last_instruction_from_exception();
            }
        }
        if protected_has_instructions {
            self.add_exception_regions_with_exclusions(
                protected_start,
                protected_end,
                &protected_exclusions,
                finally_handler,
                base_depth.cast_unsigned(),
                false,
            );
        } else {
            self.assembler.add_exception_region(
                protected_end,
                protected_end,
                finally_handler,
                base_depth.cast_unsigned(),
                false,
            );
        }

        self.assembler.mark(finally_handler);
        self.set_depth(base_depth + 1);
        self.assembler.set_location(SourceLocation::NONE);
        self.emit(PUSH_EXC_INFO, 0, 1)?;
        let handler_start = finally_handler;
        self.control
            .active_exception_region_exclusions
            .push(Vec::new());
        let mut explicit_handler_end = None;
        if let Some((Stmt::Return(return_statement), leading)) = statement.finalbody.split_last() {
            self.compile_suite(leading)?;
            explicit_handler_end =
                Some(self.compile_return_from_exception_handler(return_statement)?);
        } else if let [Stmt::If(final_if)] = statement.finalbody.as_slice()
            && final_if.elif_else_clauses.is_empty()
        {
            let condition_false = self.assembler.label();
            self.compile_jump_if(&final_if.test, false, condition_false)?;
            self.with_control_counter(
                |control| &mut control.active_finally_end_blocks,
                |compiler| compiler.compile_suite(&final_if.body),
            )?;
            if !suite_terminates(&final_if.body) {
                if let Some(location) = self.assembler.last_instruction_location() {
                    self.assembler.set_location(location);
                }
                self.emit(RERAISE, 0, -2)?;
            }
            self.assembler.mark(condition_false);
            self.set_depth(base_depth + 2);
            self.assembler
                .set_location(self.source_location(final_if.test.range()));
            self.emit(RERAISE, 0, -2)?;
        } else {
            if let Some(location) = pass_finally_location {
                self.assembler.set_location(location);
                self.emit(NOP, 0, 0)?;
            } else {
                self.with_control_counter(
                    |control| &mut control.active_finally_end_blocks,
                    |compiler| compiler.compile_suite(&statement.finalbody),
                )?;
            }
            if !suite_terminates(&statement.finalbody) {
                if let Some(statement) = statement
                    .finalbody
                    .last()
                    .filter(|statement| statement_uses_implicit_return_location(statement))
                {
                    self.assembler
                        .set_location(self.source_location(implicit_return_range(self, statement)));
                } else if let Some(location) = self.assembler.last_instruction_location() {
                    self.assembler.set_location(location);
                }
                self.emit(RERAISE, 0, -2)?;
            }
        }
        let handler_end = if let Some(handler_end) = explicit_handler_end {
            handler_end
        } else {
            let handler_end = self.assembler.label();
            self.assembler.mark(handler_end);
            handler_end
        };
        let handler_exclusions = self
            .control
            .active_exception_region_exclusions
            .pop()
            .expect("active finally handler has exclusion collector");
        self.add_exception_regions_with_exclusions(
            handler_start,
            handler_end,
            &handler_exclusions,
            cleanup,
            (base_depth + 1).cast_unsigned(),
            true,
        );

        self.assembler.mark(cleanup);
        self.set_depth(base_depth + 3);
        self.assembler.set_location(SourceLocation::NONE);
        self.emit(COPY, 3, 1)?;
        self.emit(POP_EXCEPT, 0, -1)?;
        self.emit(RERAISE, 1, -3)?;
        self.assembler.mark(end);
        self.set_depth(base_depth);
        Ok(())
    }

    fn compile_try_finally_continue_break(
        &mut self,
        statement: &ruff_python_ast::StmtTry,
    ) -> Result<(), CompileError> {
        let base_depth = self.control.depth;
        let context = self
            .control
            .loops
            .last()
            .copied()
            .ok_or_else(|| CompileError::Internal("break outside loop".to_string()))?;
        let continue_statement = &statement.body[0];
        let break_statement = &statement.finalbody[0];
        let handler = self.assembler.label();
        let handler_end = self.assembler.label();
        let cleanup = self.assembler.label();
        let empty_protected = self.assembler.label();

        self.assembler
            .set_location(self.source_location(statement.range));
        self.emit(NOP, 0, 0)?;
        self.assembler.mark(empty_protected);
        self.assembler.add_exception_region(
            empty_protected,
            empty_protected,
            handler,
            base_depth.cast_unsigned(),
            false,
        );
        self.assembler
            .set_location(self.source_location(continue_statement.range()));
        self.emit(NOP, 0, 0)?;
        self.assembler
            .set_location(self.source_location(break_statement.range()));
        match context.iterator_cleanup {
            IteratorCleanup::None => {}
            IteratorCleanup::Sync | IteratorCleanup::Async => self.emit(POP_TOP, 0, -1)?,
        }
        if context.break_returns {
            self.emit_implicit_return()?;
        } else {
            self.emit_jump_forward(JUMP_FORWARD, context.break_label, 0)?;
        }

        self.assembler.mark(handler);
        self.set_depth(base_depth + 1);
        self.assembler.set_location(SourceLocation::NONE);
        self.emit(PUSH_EXC_INFO, 0, 1)?;
        self.assembler
            .set_location(self.source_location(break_statement.range()));
        self.emit(POP_TOP, 0, -1)?;
        self.assembler.mark(handler_end);
        self.emit(POP_EXCEPT, 0, -1)?;
        match context.iterator_cleanup {
            IteratorCleanup::None => {}
            IteratorCleanup::Sync | IteratorCleanup::Async => self.emit(POP_TOP, 0, -1)?,
        }
        if context.break_returns {
            self.emit_implicit_return()?;
        } else {
            self.emit_jump_forward(JUMP_FORWARD, context.break_label, 0)?;
        }
        self.assembler.add_exception_region(
            handler,
            handler_end,
            cleanup,
            (base_depth + 1).cast_unsigned(),
            true,
        );

        self.assembler.mark(cleanup);
        self.set_depth(base_depth + 3);
        self.assembler.set_location(SourceLocation::NONE);
        self.emit(COPY, 3, 1)?;
        self.emit(POP_EXCEPT, 0, -1)?;
        self.emit(RERAISE, 1, -3)?;
        self.set_depth(base_depth);
        Ok(())
    }

    fn compile_return_from_exception_handler(
        &mut self,
        statement: &ruff_python_ast::StmtReturn,
    ) -> Result<Label, CompileError> {
        let preserve_value = statement
            .value
            .as_deref()
            .is_some_and(|value| !is_literal_constant(value));
        if preserve_value {
            self.compile_expression(
                statement
                    .value
                    .as_deref()
                    .expect("preserved return has a value"),
            )?;
            self.emit(SWAP, 2, 0)?;
        } else {
            let range = statement
                .value
                .as_deref()
                .map_or(statement.range, Ranged::range);
            self.assembler.set_location(self.source_location(range));
        }

        self.emit(POP_TOP, 0, -1)?;
        if preserve_value {
            self.emit(SWAP, 2, 0)?;
        }
        let handler_end = self.assembler.label();
        self.assembler.mark(handler_end);
        self.emit(POP_EXCEPT, 0, -1)?;
        self.emit_loop_iterator_cleanup(0, self.control.loops.len(), preserve_value)?;

        let pass_finally_unwind_start = (!self.control.active_pass_finally_locations.is_empty())
            .then(|| {
                let start = self.assembler.label();
                self.assembler.mark(start);
                start
            });
        self.emit_pass_finally_nops()?;

        if !preserve_value {
            if let Some(value) = &statement.value {
                self.compile_expression(value)?;
            } else {
                let none = self.add_constant(Constant::None)?;
                self.emit(LOAD_CONST, none, 1)?;
            }
        }
        self.emit(RETURN_VALUE, 0, -1)?;
        if let Some(pass_finally_unwind_start) = pass_finally_unwind_start {
            let unwind_end = self.assembler.label();
            self.assembler.mark(unwind_end);
            let outer_region_count = self
                .control
                .active_exception_region_exclusions
                .len()
                .saturating_sub(1);
            for exclusions in self
                .control
                .active_exception_region_exclusions
                .iter_mut()
                .take(outer_region_count)
            {
                exclusions.push((pass_finally_unwind_start, unwind_end));
            }
        }
        Ok(handler_end)
    }

    fn compile_with(&mut self, statement: &ruff_python_ast::StmtWith) -> Result<(), CompileError> {
        self.compile_with_items(&statement.items, &statement.body, false)
    }

    fn compile_with_body(
        &mut self,
        remaining: &[ruff_python_ast::WithItem],
        body: &[Stmt],
    ) -> Result<Option<ruff_text_size::TextRange>, CompileError> {
        let mut body_noop = None;
        if remaining.is_empty() {
            let body_start = self.assembler.instruction_count();
            let body_previous_location = self.assembler.last_instruction_location();
            let terminal_if = matches!(
                body.last(),
                Some(Stmt::If(statement))
                    if statement.elif_else_clauses.is_empty()
                        && suite_terminates(&statement.body)
            );
            let terminal_branching = matches!(
                body.last(),
                Some(Stmt::If(statement))
                    if statement
                        .elif_else_clauses
                        .iter()
                        .any(|clause| clause.test.is_some())
            );
            if terminal_if || terminal_branching {
                let (last, leading) = body.split_last().expect("with body has a final if");
                self.compile_suite(leading)?;
                self.with_control_override(
                    |control| &mut control.exclude_terminal_if_not_taken,
                    true,
                    |compiler| compiler.compile_statement(last),
                )?;
            } else if !matches!(body, [Stmt::Pass(_)]) {
                // Keep direct pass-only bodies on the with compiler's fallback path. CPython
                // retains that NOP inside the protected range, unlike a terminal pass in a
                // nested suite such as a loop `else`.
                self.compile_suite(body)?;
            }
            let trailing_nop_is_covered = body_previous_location.is_some_and(|previous| {
                previous.line >= 0
                    && self
                        .assembler
                        .last_instruction_location()
                        .is_some_and(|last| last.line == previous.line)
            }) && self.take_trailing_nop_location().is_some();
            if !trailing_nop_is_covered
                && self.assembler.instruction_count() == body_start
                && let Some(statement) = body.last()
                // A same-line `as` target store already carries the pass line event.
                && body_previous_location.is_none_or(|previous| {
                    previous.line != self.source_location(statement.range()).line
                })
            {
                body_noop = Some(statement.range());
            } else if terminal_if && let Some(Stmt::If(statement)) = body.last() {
                body_noop = Some(statement.test.range());
            }
        } else {
            self.compile_with_items(remaining, body, false)?;
        }
        Ok(body_noop)
    }

    fn compile_with_items(
        &mut self,
        items: &[ruff_python_ast::WithItem],
        body: &[Stmt],
        terminal: bool,
    ) -> Result<(), CompileError> {
        let Some((item, remaining)) = items.split_first() else {
            return self.compile_suite(body);
        };
        let base_depth = self.control.depth;
        let protected_start = self.assembler.label();
        let protected_end = self.assembler.label();
        let handler = self.assembler.label();
        let suppress = self.assembler.label();
        let handler_end = self.assembler.label();
        let cleanup = self.assembler.label();
        let end = self.assembler.label();
        self.control.max_depth = self.control.max_depth.max((base_depth + 7).cast_unsigned());
        let newly_initialized_targets =
            item.optional_vars
                .as_deref()
                .map_or_else(Vec::new, |target| {
                    let mut names = Vec::new();
                    collect_target_names(target, &mut names);
                    names.retain(|name| !self.is_initialized(name));
                    names
                });

        self.compile_expression(&item.context_expr)?;
        self.assembler
            .set_location(self.source_location(item.context_expr.range()));
        self.emit(COPY, 1, 1)?;
        self.emit_special_method(SpecialMethod::__EXIT__)?;
        self.emit(SWAP, 2, 0)?;
        self.emit(SWAP, 3, 0)?;
        self.emit_special_method(SpecialMethod::__ENTER__)?;
        self.emit(CALL, 0, -1)?;
        self.assembler.mark(protected_start);
        if let Some(target) = &item.optional_vars {
            self.compile_store_target(target)?;
        } else {
            self.emit(POP_TOP, 0, -1)?;
        }
        let context = WithExitContext {
            location: self.source_location(item.context_expr.range()),
            is_async: false,
        };
        let (body_noop, region_exclusions) = self.with_region_exclusion_collector(
            |control| &mut control.active_with_region_exclusions,
            |compiler| {
                compiler.with_with_context(context, terminal, |compiler| {
                    compiler.compile_with_body(remaining, body)
                })
            },
        )?;
        self.assembler.mark(protected_end);
        if !remaining.is_empty() && suite_terminates(body) {
            // A nested manager can suppress the terminal body's exception. CPython keeps the
            // nested item's line marker before running this outer manager's normal exit.
            self.assembler
                .set_location(self.source_location(remaining[0].context_expr.range()));
            self.emit(NOP, 0, 0)?;
        }
        if let Some(range) = body_noop {
            let noop_start = self.assembler.label();
            self.assembler.mark(noop_start);
            self.assembler.set_location(self.source_location(range));
            self.emit(NOP, 0, 0)?;
            let noop_end = self.assembler.label();
            self.assembler.mark(noop_end);
            self.control
                .generator_region_exclusions
                .push((noop_start, noop_end));
            for exclusions in &mut self.control.active_with_region_exclusions {
                exclusions.push((noop_start, noop_end));
            }
            for exclusions in &mut self.control.active_exception_region_exclusions {
                exclusions.push((noop_start, noop_end));
            }
        }

        self.assembler
            .set_location(self.source_location(item.context_expr.range()));
        let none = self.add_constant(Constant::None)?;
        if !suite_terminates(body) || !remaining.is_empty() {
            self.emit(LOAD_CONST, none, 1)?;
            self.emit(LOAD_CONST, none, 1)?;
            self.emit(LOAD_CONST, none, 1)?;
            self.emit(CALL, 3, -4)?;
            self.emit(POP_TOP, 0, -1)?;
            if terminal {
                self.emit_implicit_return()?;
            } else {
                self.emit_jump_forward(JUMP_FORWARD, end, 0)?;
                if self.control.active_finally_try_bodies > 0 {
                    self.assembler.prevent_last_jump_inlining();
                }
            }
        }
        let mut region_start = protected_start;
        for (exclusion_start, exclusion_end) in region_exclusions {
            self.assembler.add_exception_region(
                region_start,
                exclusion_start,
                handler,
                (base_depth + 2).cast_unsigned(),
                true,
            );
            region_start = exclusion_end;
        }
        self.assembler.add_exception_region(
            region_start,
            protected_end,
            handler,
            (base_depth + 2).cast_unsigned(),
            true,
        );

        self.assembler.mark(handler);
        self.set_depth(base_depth + 4);
        self.assembler
            .set_location(self.source_location(item.context_expr.range()));
        self.emit(PUSH_EXC_INFO, 0, 1)?;
        self.emit(WITH_EXCEPT_START, 0, 1)?;
        self.emit(TO_BOOL, 0, 0)?;
        self.emit_jump_forward(POP_JUMP_IF_TRUE, suppress, -1)?;
        self.emit(NOT_TAKEN, 0, 0)?;
        self.emit(RERAISE, 2, -5)?;
        self.assembler.mark(suppress);
        self.set_depth(base_depth + 5);
        self.emit(POP_TOP, 0, -1)?;
        self.assembler.mark(handler_end);
        self.assembler.add_exception_region(
            handler,
            handler_end,
            cleanup,
            (base_depth + 4).cast_unsigned(),
            true,
        );
        self.emit(POP_EXCEPT, 0, -1)?;
        self.emit(POP_TOP, 0, -1)?;
        self.emit(POP_TOP, 0, -1)?;
        self.emit(POP_TOP, 0, -1)?;
        if terminal {
            self.emit_implicit_return()?;
        } else {
            self.emit_jump_forward(JUMP_FORWARD, end, 0)?;
            if self.control.active_finally_try_bodies > 0 {
                self.assembler.prevent_last_jump_inlining();
            }
        }

        self.assembler.mark(cleanup);
        self.set_depth(base_depth + 3);
        self.assembler.set_location(SourceLocation::NONE);
        self.emit(COPY, 3, 1)?;
        self.emit(POP_EXCEPT, 0, -1)?;
        self.emit(RERAISE, 1, -3)?;
        self.assembler.mark(end);
        self.set_depth(base_depth);
        for name in newly_initialized_targets {
            let name = self.required_name_id(&name);
            self.symbols.initialized_locals.remove(name);
        }
        Ok(())
    }

    fn compile_async_with(
        &mut self,
        statement: &ruff_python_ast::StmtWith,
    ) -> Result<(), CompileError> {
        if self.unit.flags & (CO_COROUTINE | CO_ASYNC_GENERATOR) == 0 {
            return Err(unsupported("async with outside coroutine code"));
        }
        self.compile_async_with_items(&statement.items, &statement.body, false)
    }

    fn compile_async_with_items(
        &mut self,
        items: &[ruff_python_ast::WithItem],
        body: &[Stmt],
        terminal: bool,
    ) -> Result<(), CompileError> {
        let Some((item, remaining)) = items.split_first() else {
            return self.compile_suite(body);
        };
        let base_depth = self.control.depth;
        let protected_start = self.assembler.label();
        let protected_end = self.assembler.label();
        let handler = self.assembler.label();
        let suppress = self.assembler.label();
        let handler_end = self.assembler.label();
        let cleanup = self.assembler.label();
        let end = self.assembler.label();
        let newly_initialized_targets =
            item.optional_vars
                .as_deref()
                .map_or_else(Vec::new, |target| {
                    let mut names = Vec::new();
                    collect_target_names(target, &mut names);
                    names.retain(|name| !self.is_initialized(name));
                    names
                });

        self.compile_expression(&item.context_expr)?;
        self.assembler
            .set_location(self.source_location(item.context_expr.range()));
        self.emit(COPY, 1, 1)?;
        self.emit_special_method(SpecialMethod::__AEXIT__)?;
        self.emit(SWAP, 2, 0)?;
        self.emit(SWAP, 3, 0)?;
        self.emit_special_method(SpecialMethod::__AENTER__)?;
        self.emit(CALL, 0, -1)?;
        self.compile_awaitable_on_stack(1)?;
        self.assembler.mark(protected_start);
        let context = WithExitContext {
            location: self.source_location(item.context_expr.range()),
            is_async: true,
        };
        let ((), mut region_exclusions) = self.with_region_exclusion_collector(
            |control| &mut control.active_with_region_exclusions,
            |compiler| {
                if let Some(target) = &item.optional_vars {
                    compiler.compile_store_target(target)?;
                } else {
                    compiler.emit(POP_TOP, 0, -1)?;
                }
                compiler.with_with_context(context, false, |compiler| {
                    let body_start = compiler.assembler.instruction_count();
                    if remaining.is_empty() {
                        if !matches!(body, [Stmt::Pass(_)]) {
                            compiler.compile_suite(body)?;
                        }
                    } else {
                        compiler.compile_async_with_items(remaining, body, false)?;
                    }
                    if remaining.is_empty()
                        && compiler.assembler.instruction_count() == body_start
                        && let Some(statement) = body.last()
                    {
                        compiler
                            .assembler
                            .set_location(compiler.source_location(statement.range()));
                        compiler.emit(NOP, 0, 0)?;
                    }
                    Ok(())
                })
            },
        )?;
        if remaining.is_empty()
            && matches!(body, [Stmt::Expr(expression)] if fold_constant(&expression.value).is_some())
        {
            region_exclusions.clear();
        }
        self.assembler.mark(protected_end);

        self.assembler
            .set_location(self.source_location(item.context_expr.range()));
        let none = self.add_constant(Constant::None)?;
        self.emit(LOAD_CONST, none, 1)?;
        self.emit(LOAD_CONST, none, 1)?;
        self.emit(LOAD_CONST, none, 1)?;
        self.emit(CALL, 3, -4)?;
        self.compile_awaitable_on_stack(2)?;
        self.emit(POP_TOP, 0, -1)?;
        if terminal {
            self.emit_implicit_return()?;
        } else {
            self.emit_jump_forward(JUMP_FORWARD, end, 0)?;
        }
        let mut region_start = protected_start;
        for (exclusion_start, exclusion_end) in region_exclusions {
            self.assembler.add_exception_region(
                region_start,
                exclusion_start,
                handler,
                (base_depth + 2).cast_unsigned(),
                true,
            );
            region_start = exclusion_end;
        }
        self.assembler.add_exception_region(
            region_start,
            protected_end,
            handler,
            (base_depth + 2).cast_unsigned(),
            true,
        );

        self.assembler.mark(handler);
        self.set_depth(base_depth + 4);
        self.assembler
            .set_location(self.source_location(item.context_expr.range()));
        self.emit(PUSH_EXC_INFO, 0, 1)?;
        self.emit(WITH_EXCEPT_START, 0, 1)?;
        self.compile_awaitable_on_stack(2)?;
        self.emit(TO_BOOL, 0, 0)?;
        self.emit_jump_forward(POP_JUMP_IF_TRUE, suppress, -1)?;
        let not_taken_start = self.assembler.label();
        self.assembler.mark(not_taken_start);
        self.emit(NOT_TAKEN, 0, 0)?;
        let not_taken_end = self.assembler.label();
        self.assembler.mark(not_taken_end);
        self.control
            .generator_region_exclusions
            .push((not_taken_start, not_taken_end));
        for exclusions in &mut self.control.active_with_region_exclusions {
            exclusions.push((not_taken_start, not_taken_end));
        }
        self.emit(RERAISE, 2, -5)?;
        self.assembler.mark(suppress);
        self.set_depth(base_depth + 5);
        self.emit(POP_TOP, 0, -1)?;
        self.assembler.mark(handler_end);
        self.assembler.add_exception_region(
            handler,
            not_taken_start,
            cleanup,
            (base_depth + 4).cast_unsigned(),
            true,
        );
        self.assembler.add_exception_region(
            not_taken_end,
            handler_end,
            cleanup,
            (base_depth + 4).cast_unsigned(),
            true,
        );
        self.emit(POP_EXCEPT, 0, -1)?;
        self.emit(POP_TOP, 0, -1)?;
        self.emit(POP_TOP, 0, -1)?;
        self.emit(POP_TOP, 0, -1)?;
        if terminal {
            self.emit_implicit_return()?;
        } else {
            self.emit_jump_forward(JUMP_FORWARD, end, 0)?;
        }

        self.assembler.mark(cleanup);
        self.set_depth(base_depth + 3);
        self.assembler.set_location(SourceLocation::NONE);
        self.emit(COPY, 3, 1)?;
        self.emit(POP_EXCEPT, 0, -1)?;
        self.emit(RERAISE, 1, -3)?;
        self.assembler.mark(end);
        self.set_depth(base_depth);
        for name in newly_initialized_targets {
            let name = self.required_name_id(&name);
            self.symbols.initialized_locals.remove(name);
        }
        Ok(())
    }

    fn compile_import(
        &mut self,
        statement: &ruff_python_ast::StmtImport,
    ) -> Result<(), CompileError> {
        for alias in &statement.names {
            if self.output.constants.is_empty() {
                self.add_constant(Constant::Int(0))?;
            }
            self.emit(LOAD_SMALL_INT, 0, 1)?;
            let none = self.add_constant(Constant::None)?;
            self.emit(LOAD_CONST, none, 1)?;
            let module = alias.name.as_str();
            let module_index = self.name_index(module)?;
            self.emit(IMPORT_NAME, module_index, -1)?;

            if let Some(as_name) = &alias.asname {
                self.compile_import_as(module, as_name.as_str())?;
            } else {
                let bound_name = module.split('.').next().unwrap_or(module);
                self.store_name(bound_name)?;
            }
        }
        Ok(())
    }

    fn compile_import_as(&mut self, module: &str, as_name: &str) -> Result<(), CompileError> {
        let mut parts = module.split('.');
        let _ = parts.next();
        let remaining: Vec<_> = parts.collect();
        if remaining.is_empty() {
            return self.store_name(as_name);
        }

        for (position, part) in remaining.iter().enumerate() {
            let index = self.name_index(part)?;
            self.emit(IMPORT_FROM, index, 1)?;
            if position + 1 < remaining.len() {
                self.emit(SWAP, 2, 0)?;
                self.emit(POP_TOP, 0, -1)?;
            }
        }
        self.store_name(as_name)?;
        self.emit(POP_TOP, 0, -1)
    }

    fn compile_from_import(
        &mut self,
        statement: &ruff_python_ast::StmtImportFrom,
    ) -> Result<(), CompileError> {
        if self.output.constants.is_empty() {
            self.add_constant(Constant::Int(u64::from(statement.level)))?;
        }
        self.emit(LOAD_SMALL_INT, statement.level, 1)?;
        let imported_names = statement
            .names
            .iter()
            .map(|alias| Constant::String(alias.name.as_str().to_string()))
            .collect();
        let names = self.add_constant(Constant::Tuple(imported_names))?;
        self.emit(LOAD_CONST, names, 1)?;
        let module = statement.module.as_ref().map_or("", |name| name.as_str());
        let module_index = self.name_index(module)?;
        self.emit(IMPORT_NAME, module_index, -1)?;

        if statement.names.len() == 1 && statement.names[0].name.as_str() == "*" {
            self.emit_unary_intrinsic(UnaryIntrinsic::IMPORT_STAR)?;
            self.emit(POP_TOP, 0, -1)?;
            return Ok(());
        }

        for alias in &statement.names {
            let index = self.name_index(alias.name.as_str())?;
            self.emit(IMPORT_FROM, index, 1)?;
            let bound_name = alias
                .asname
                .as_ref()
                .map_or(alias.name.as_str(), |name| name.as_str());
            self.store_name(bound_name)?;
        }
        self.emit(POP_TOP, 0, -1)
    }
}

pub(super) fn suite_terminates(body: &[Stmt]) -> bool {
    body.iter().any(statement_terminates)
}

fn try_requires_loop_tail_inlining(statement: &ruff_python_ast::StmtTry) -> bool {
    !statement.is_star
        && statement.finalbody.is_empty()
        && statement.orelse.is_empty()
        && matches!(
            statement.body.last(),
            Some(Stmt::If(statement))
                if statement.elif_else_clauses.is_empty()
                    && !suite_terminates(&statement.body)
        )
}

pub(super) fn suite_contains_yield(body: &[Stmt]) -> bool {
    #[derive(Default)]
    struct YieldCollector {
        found: bool,
    }

    impl<'ast> Visitor<'ast> for YieldCollector {
        fn visit_stmt(&mut self, statement: &'ast Stmt) {
            match statement {
                Stmt::FunctionDef(_) => return,
                Stmt::ClassDef(definition) => {
                    for decorator in &definition.decorator_list {
                        self.visit_expr(&decorator.expression);
                    }
                    if let Some(arguments) = definition.arguments.as_deref() {
                        for argument in &arguments.args {
                            self.visit_expr(argument);
                        }
                        for keyword in &arguments.keywords {
                            self.visit_expr(&keyword.value);
                        }
                    }
                    return;
                }
                _ => {}
            }
            walk_stmt(self, statement);
        }

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

    let mut collector = YieldCollector::default();
    for statement in body {
        collector.visit_stmt(statement);
    }
    collector.found
}
fn statement_terminates(statement: &Stmt) -> bool {
    match statement {
        Stmt::Break(_) | Stmt::Continue(_) | Stmt::Return(_) | Stmt::Raise(_) => true,
        Stmt::While(statement) => {
            early_condition_truthiness(&statement.test) == Some(true)
                && !suite_contains_loop_break(&statement.body)
        }
        Stmt::For(statement) => {
            !suite_contains_loop_break(&statement.body) && suite_terminates(&statement.orelse)
        }
        Stmt::If(statement) => {
            suite_terminates(&statement.body)
                && !statement.elif_else_clauses.is_empty()
                && statement
                    .elif_else_clauses
                    .iter()
                    .all(|clause| suite_terminates(&clause.body))
                && statement
                    .elif_else_clauses
                    .last()
                    .is_some_and(|clause| clause.test.is_none())
        }
        Stmt::Try(statement) => suite_terminates(&statement.finalbody),
        _ => false,
    }
}

fn terminating_if_fallthrough_test(statement: &ruff_python_ast::StmtIf) -> Option<&Expr> {
    if !suite_terminates(&statement.body) {
        return None;
    }
    let mut final_test = statement.test.as_ref();
    for clause in &statement.elif_else_clauses {
        let test = clause.test.as_ref()?;
        if !suite_terminates(&clause.body) {
            return None;
        }
        final_test = test;
    }
    Some(final_test)
}

fn terminal_exception_handler_if_supported(statement: &ruff_python_ast::StmtIf) -> bool {
    std::iter::once(statement.body.as_slice())
        .chain(
            statement
                .elif_else_clauses
                .iter()
                .map(|clause| clause.body.as_slice()),
        )
        .all(|body| {
            !suite_terminates(body)
                || matches!(body, [Stmt::Return(statement)] if statement.value.is_none())
        })
}

pub(super) fn statement_uses_implicit_return_location(statement: &Stmt) -> bool {
    matches!(statement, Stmt::Assign(assignment) if assignment.targets.len() > 1 || matches!(assignment.targets.last(), Some(Expr::List(_) | Expr::Tuple(_))))
        || matches!(statement, Stmt::Assign(assignment) if matches!(assignment.value.as_ref(), Expr::ListComp(_) | Expr::SetComp(_) | Expr::DictComp(_)))
        || matches!(statement, Stmt::Expr(expression) if fold_constant(&expression.value).is_some())
        || matches!(
            statement,
            Stmt::AnnAssign(assignment)
                if assignment.value.is_none()
                    && matches!(assignment.target.as_ref(), Expr::Subscript(_))
        )
        || matches!(statement, Stmt::While(statement) if statement.orelse.is_empty())
        || matches!(
            statement,
            Stmt::For(statement)
                if statement.orelse.is_empty()
                    && !suite_contains_loop_break(&statement.body)
        )
}
pub(super) fn is_wildcard_pattern(pattern: &Pattern) -> bool {
    matches!(
        pattern,
        Pattern::MatchAs(pattern) if pattern.pattern.is_none() && pattern.name.is_none()
    )
}

fn suite_contains_loop_break(body: &[Stmt]) -> bool {
    body.iter().any(|statement| match statement {
        Stmt::Break(_) => true,
        Stmt::If(statement) => {
            suite_contains_loop_break(&statement.body)
                || statement
                    .elif_else_clauses
                    .iter()
                    .any(|clause| suite_contains_loop_break(&clause.body))
        }
        Stmt::Try(statement) => {
            suite_contains_loop_break(&statement.body)
                || statement
                    .handlers
                    .iter()
                    .filter_map(ruff_python_ast::ExceptHandler::as_except_handler)
                    .any(|handler| suite_contains_loop_break(&handler.body))
                || suite_contains_loop_break(&statement.orelse)
                || suite_contains_loop_break(&statement.finalbody)
        }
        Stmt::With(statement) => suite_contains_loop_break(&statement.body),
        Stmt::Match(statement) => statement
            .cases
            .iter()
            .any(|case| suite_contains_loop_break(&case.body)),
        Stmt::For(statement) => suite_contains_loop_break(&statement.orelse),
        Stmt::While(statement) => suite_contains_loop_break(&statement.orelse),
        Stmt::FunctionDef(_) | Stmt::ClassDef(_) => false,
        _ => false,
    })
}
