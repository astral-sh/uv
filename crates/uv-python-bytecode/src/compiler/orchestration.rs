// Compiler submodules intentionally share the parent-owned state and lowering vocabulary.
#![allow(clippy::wildcard_imports)]

use super::*;

impl Compiler {
    fn new(context: Arc<CompilationContext>, unit: CodeUnit, symbols: SymbolState) -> Self {
        Self {
            context,
            unit,
            output: OutputState::default(),
            symbols,
            control: ControlFlowState::default(),
            assembler: Assembler::default(),
        }
    }

    pub(crate) fn module(filename: &str, source: &str) -> Self {
        let context = Arc::new(CompilationContext {
            filename: filename.to_string(),
            source: Arc::from(source),
            line_index: LineIndex::from_source_text(source),
            scopes: OnceLock::new(),
        });
        let unit = CodeUnit::from_spec(CodeUnitSpec {
            scope: Scope::Module,
            scope_id: ScopeId::MODULE,
            name: "<module>".to_string(),
            qualified_name: "<module>".to_string(),
            private_name: None,
            arg_count: 0,
            positional_only_arg_count: 0,
            keyword_only_arg_count: 0,
            flags: 0,
            first_line_number: 1,
        });
        Self::new(context, unit, SymbolState::default())
    }

    pub(super) fn function(
        context: Arc<CompilationContext>,
        name: &str,
        qualified_name: String,
        first_line_number: u32,
        plan: &LoweringFunctionPlan,
        scope_id: ScopeId,
        arg_count: u32,
        positional_only_arg_count: u32,
        keyword_only_arg_count: u32,
        parameter_flags: u32,
    ) -> Result<Self, CompileError> {
        let parameter_count = usize::try_from(
            arg_count
                + keyword_only_arg_count
                + u32::from(parameter_flags & CO_VARARGS != 0)
                + u32::from(parameter_flags & CO_VARKEYWORDS != 0),
        )
        .unwrap_or(usize::MAX);
        let mut locals: Vec<_> = plan
            .locals
            .iter()
            .enumerate()
            .filter(|(index, name)| {
                *index < parameter_count
                    || !plan.cellvars.contains(*name)
                    || plan.inlined_comprehension_cellvars.contains(*name)
            })
            .map(|(_, name)| name.clone())
            .collect();
        let fast_local_count = locals.len();
        let mut cell_only_locals: Vec<_> = plan
            .locals
            .iter()
            .enumerate()
            .filter(|(index, name)| {
                *index >= parameter_count
                    && plan.cellvars.contains(*name)
                    && !plan.inlined_comprehension_cellvars.contains(*name)
            })
            .map(|(_, name)| name.clone())
            .collect();
        cell_only_locals.sort();
        locals.extend(cell_only_locals);
        let names = &context
            .scopes
            .get()
            .expect("scope arena should be initialized before child compilation")
            .names;
        let indices = locals
            .iter()
            .enumerate()
            .map(|(index, name)| {
                let name = names
                    .get(name)
                    .unwrap_or_else(|| panic!("module analysis did not intern `{name}`"));
                Ok((name, to_u32(index, "local variable count")?))
            })
            .collect::<Result<FxHashMap<_, _>, CompileError>>()?;
        let local_count = locals.len();
        let free_indices = plan
            .freevars
            .iter()
            .enumerate()
            .map(|(index, name)| {
                let name = names
                    .get(name)
                    .unwrap_or_else(|| panic!("module analysis did not intern `{name}`"));
                Ok((name, to_u32(local_count + index, "free variable count")?))
            })
            .collect::<Result<FxHashMap<_, _>, CompileError>>()?;
        locals.extend(plan.freevars.iter().cloned());

        let cell_names: NameSet = plan
            .cellvars
            .iter()
            .map(|name| {
                names
                    .get(name)
                    .unwrap_or_else(|| panic!("module analysis did not intern `{name}`"))
            })
            .collect();
        let global_names = plan
            .globals
            .iter()
            .map(|name| {
                names
                    .get(name)
                    .unwrap_or_else(|| panic!("module analysis did not intern `{name}`"))
            })
            .collect();
        let initialized_locals = locals
            .iter()
            .take(parameter_count)
            .map(|name| {
                names
                    .get(name)
                    .unwrap_or_else(|| panic!("module analysis did not intern `{name}`"))
            })
            .collect();

        let symbols = SymbolState {
            locals,
            fast_local_count,
            cell_names: cell_names.clone(),
            free_names: plan.freevars.iter().cloned().collect(),
            initialized_locals,
            ..SymbolState::default()
        };
        let unit = CodeUnit::from_spec(CodeUnitSpec {
            scope: Scope::Function {
                indices,
                free_indices,
                cells: cell_names,
                globals: global_names,
            },
            scope_id,
            name: name.to_string(),
            qualified_name,
            private_name: None,
            arg_count,
            positional_only_arg_count,
            keyword_only_arg_count,
            flags: CO_OPTIMIZED | CO_NEWLOCALS | parameter_flags,
            first_line_number,
        });
        Ok(Self::new(context, unit, symbols))
    }

    pub(super) fn class(
        context: Arc<CompilationContext>,
        name: &str,
        qualified_name: String,
        first_line_number: u32,
        scope_id: ScopeId,
        globals: HashSet<String>,
        nonlocals: HashSet<String>,
        freevars: BTreeSet<String>,
        bound_names: HashSet<String>,
        needs_class_closure: bool,
        has_methods: bool,
        future_flags: u32,
    ) -> Self {
        let mut locals = Vec::new();
        let mut cell_names = NameSet::default();
        let names = &context
            .scopes
            .get()
            .expect("scope arena should be initialized before child compilation")
            .names;
        if needs_class_closure {
            locals.push("__class__".to_string());
            cell_names.insert(
                names
                    .get("__class__")
                    .expect("module analysis should intern `__class__`"),
            );
        }
        if has_methods {
            locals.push("__classdict__".to_string());
            cell_names.insert(
                names
                    .get("__classdict__")
                    .expect("module analysis should intern `__classdict__`"),
            );
        }
        let free_start = locals.len();
        let free_indices = freevars
            .iter()
            .enumerate()
            .map(|(index, name)| {
                let name = names
                    .get(name)
                    .unwrap_or_else(|| panic!("module analysis did not intern `{name}`"));
                (name, u32::try_from(free_start + index).unwrap_or(u32::MAX))
            })
            .collect();
        let free_names: Vec<_> = freevars.into_iter().collect();
        locals.extend(free_names.iter().cloned());
        let globals = globals
            .into_iter()
            .map(|name| {
                names
                    .get(&name)
                    .unwrap_or_else(|| panic!("module analysis did not intern `{name}`"))
            })
            .collect();
        let nonlocals = nonlocals
            .into_iter()
            .map(|name| {
                names
                    .get(&name)
                    .unwrap_or_else(|| panic!("module analysis did not intern `{name}`"))
            })
            .collect();
        let bound_names = bound_names
            .into_iter()
            .map(|name| {
                names
                    .get(&name)
                    .unwrap_or_else(|| panic!("module analysis did not intern `{name}`"))
            })
            .collect();
        let symbols = SymbolState {
            locals,
            cell_names,
            free_names,
            ..SymbolState::default()
        };
        let unit = CodeUnit::from_spec(CodeUnitSpec {
            scope: Scope::Class {
                globals,
                nonlocals,
                free_indices,
                bound_names,
            },
            scope_id,
            name: name.to_string(),
            qualified_name,
            private_name: Some(name.to_string()),
            arg_count: 0,
            positional_only_arg_count: 0,
            keyword_only_arg_count: 0,
            flags: future_flags,
            first_line_number,
        });
        Self::new(context, unit, symbols)
    }

    pub(crate) fn compile_module(mut self, body: &Suite) -> Result<CodeObject, CompileError> {
        let future_flags = future_feature_flags(body);
        let arena = ScopeArena::analyze(body, future_flags & CO_FUTURE_ANNOTATIONS != 0);
        if self.context.scopes.set(arena).is_err() {
            return Err(CompileError::Internal(
                "scope arena was initialized more than once".to_string(),
            ));
        }
        self.symbols.module_globals = module_global_names(body)
            .into_iter()
            .map(|name| self.required_name_id(&name))
            .collect();
        self.symbols.imported_scope_names = module_imported_names(body)
            .into_iter()
            .map(|name| self.required_name_id(&name))
            .collect();
        let mut module_bindings = LocalCollector::default();
        module_bindings.collect_suite(body);
        for name in &module_bindings.comprehension_targets {
            let index = to_u32(self.symbols.locals.len(), "module local count")?;
            self.symbols.locals.push(name.clone());
            let name = self.required_name_id(name);
            self.symbols.temporary_indices.insert(name, index);
            self.symbols.hidden_names.insert(name);
        }
        let comprehension_cell_names = inlined_comprehension_cell_names_in_suite(body)
            .into_iter()
            .map(|name| self.required_name_id(&name))
            .collect::<Vec<_>>();
        self.symbols.cell_names.extend(comprehension_cell_names);
        self.symbols.fast_local_count = self.symbols.locals.len();
        self.assembler.set_location(SourceLocation::NONE);
        let comprehension_cells = self
            .symbols
            .locals
            .iter()
            .enumerate()
            .filter(|(_, name)| self.is_cell(name))
            .map(|(index, _)| index)
            .collect::<Vec<_>>();
        for index in comprehension_cells {
            self.emit(MAKE_CELL, to_u32(index, "module cell index")?, 0)?;
        }
        if module_bindings.seen.contains("super") {
            // This set is propagated to every child compiler. An impossible
            // Python identifier keeps the module-shadowing fact alongside the
            // imported-name facts used by the same optimization checks.
            let sentinel = self.required_name_id(SHADOWED_SUPER_SENTINEL);
            self.symbols.imported_scope_names.insert(sentinel);
        }
        self.unit.flags |= future_flags;
        let simple_module_annotations = has_simple_annotations(body);
        let module_annotations = has_annotations(body);
        let future_module_annotations =
            self.unit.flags & CO_FUTURE_ANNOTATIONS != 0 && module_annotations;
        if module_annotations {
            let annotation_cell =
                to_u32(self.symbols.locals.len(), "module annotation cell index")?;
            self.symbols
                .locals
                .push("__conditional_annotations__".to_string());
            let annotation_name = self.required_name_id("__conditional_annotations__");
            self.symbols.cell_names.insert(annotation_name);
            self.assembler.set_location(SourceLocation::NONE);
            self.emit(MAKE_CELL, annotation_cell, 0)?;
        }
        self.assembler.set_location(SourceLocation::new(0, 1, 0, 0));
        self.emit_resume(ResumeLocation::AT_FUNC_START, false)?;
        if module_annotations {
            self.name_index("__conditional_annotations__")?;
            if let Some(statement) = body.first() {
                self.assembler
                    .set_location(self.source_location(statement.range()));
            }
            if !future_module_annotations {
                if simple_module_annotations {
                    let annotation_child = self.compile_module_annotations(body)?;
                    self.emit_deferred_constant_before_return(Constant::Code(Box::new(
                        annotation_child,
                    )))?;
                    self.emit(MAKE_FUNCTION, 0, 0)?;
                    self.emit_deferred_store_name("__annotate__")?;
                }
            }
            self.emit(BUILD_SET, 0, 1)?;
            self.store_name("__conditional_annotations__")?;
            if future_module_annotations {
                self.emit(SETUP_ANNOTATIONS, 0, 0)?;
            }
        }

        let body = if let Some(Stmt::Expr(expression)) = body.first() {
            if let Expr::StringLiteral(string) = expression.value.as_ref() {
                let constant =
                    self.add_constant(Constant::String(clean_doc(string.value.to_str())))?;
                self.assembler
                    .set_location(self.source_location(expression.range));
                self.emit(LOAD_CONST, constant, 1)?;
                self.store_name("__doc__")?;
                &body[1..]
            } else if module_annotations && fold_constant(&expression.value).is_some() {
                &body[1..]
            } else {
                body.as_slice()
            }
        } else {
            body.as_slice()
        };

        let body_start = self.assembler.instruction_count();
        let add_implicit_return = !suite_terminates(body);
        self.compile_suite_inner(body, add_implicit_return)?;
        let add_implicit_return = add_implicit_return && !self.control.emitted_fallthrough_return;
        if add_implicit_return
            && body
                .last()
                .is_some_and(statement_uses_implicit_return_location)
        {
            self.assembler.set_location(
                self.source_location(implicit_return_range(&self, body.last().unwrap())),
            );
        } else if self.assembler.instruction_count() > body_start {
            self.assembler.set_location(SourceLocation::NONE);
        } else if let Some(statement) = body
            .last()
            .filter(|statement| !matches!(statement, Stmt::Global(_) | Stmt::Nonlocal(_)))
        {
            self.assembler
                .set_location(self.source_location(statement.range()));
        }
        self.finish_inner(add_implicit_return)
    }

    pub(super) fn compile_function_body(
        mut self,
        body: &Suite,
    ) -> Result<CodeObject, CompileError> {
        self.emit_function_prologue()?;

        let body = if let Some(Stmt::Expr(expression)) = body.first() {
            if let Expr::StringLiteral(string) = expression.value.as_ref() {
                self.add_constant(Constant::String(clean_doc(string.value.to_str())))?;
                self.unit.flags |= CO_HAS_DOCSTRING;
                &body[1..]
            } else {
                body.as_slice()
            }
        } else {
            body.as_slice()
        };

        let body_start = self.assembler.instruction_count();
        let add_implicit_return = !suite_terminates(body);
        self.compile_suite_inner(body, add_implicit_return)?;
        let add_implicit_return = add_implicit_return && !self.control.emitted_fallthrough_return;
        if add_implicit_return
            && body
                .last()
                .is_some_and(statement_uses_implicit_return_location)
        {
            self.assembler.set_location(
                self.source_location(implicit_return_range(&self, body.last().unwrap())),
            );
        } else if self.assembler.instruction_count() > body_start {
            self.assembler.set_location(SourceLocation::NONE);
        } else if let Some(statement @ Stmt::Pass(_)) = body.last() {
            self.assembler
                .set_location(self.source_location(statement.range()));
        }
        self.finish_inner(add_implicit_return)
    }

    pub(super) fn compile_class_body(mut self, body: &Suite) -> Result<CodeObject, CompileError> {
        let has_classdict_cell = self.is_cell("__classdict__");
        let has_class_cell = self.is_cell("__class__");
        self.assembler.set_location(SourceLocation::NONE);
        if !self.symbols.free_names.is_empty() {
            self.emit(
                COPY_FREE_VARS,
                to_u32(self.symbols.free_names.len(), "class free variable count")?,
                0,
            )?;
        }
        let cell_indices = self
            .symbols
            .locals
            .iter()
            .take(self.symbols.locals.len() - self.symbols.free_names.len())
            .enumerate()
            .filter(|(_, name)| self.is_cell(name))
            .map(|(index, _)| index)
            .collect::<Vec<_>>();
        for index in cell_indices {
            self.emit(MAKE_CELL, to_u32(index, "class cell index")?, 0)?;
        }
        let line = i32::try_from(self.unit.first_line_number).unwrap_or(i32::MAX);
        self.assembler
            .set_location(SourceLocation::new(line, line, 0, 0));
        self.emit_resume(ResumeLocation::AT_FUNC_START, false)?;
        self.load_name("__name__")?;
        self.store_name("__module__")?;
        let qualified_name =
            self.add_constant(Constant::String(self.unit.qualified_name.clone()))?;
        self.emit(LOAD_CONST, qualified_name, 1)?;
        self.store_name("__qualname__")?;
        if u8::try_from(self.unit.first_line_number).is_ok() {
            self.emit(LOAD_SMALL_INT, self.unit.first_line_number, 1)?;
        } else {
            let first_line_number =
                self.add_constant(Constant::Int(u64::from(self.unit.first_line_number)))?;
            self.emit(LOAD_CONST, first_line_number, 1)?;
        }
        self.store_name("__firstlineno__")?;
        if self
            .symbols
            .free_names
            .iter()
            .any(|name| name == ".type_params")
        {
            self.load_name(".type_params")?;
            self.store_name("__type_params__")?;
        }
        if has_classdict_cell {
            self.emit(LOAD_LOCALS, 0, 1)?;
            let index = self
                .symbols
                .locals
                .iter()
                .position(|name| name == "__classdict__")
                .expect("classdict cell is present");
            self.emit(STORE_DEREF, to_u32(index, "classdict cell index")?, -1)?;
        }
        if self.unit.flags & CO_FUTURE_ANNOTATIONS != 0 && has_simple_annotations(body) {
            self.emit(SETUP_ANNOTATIONS, 0, 0)?;
        }

        let body = if let Some(Stmt::Expr(expression)) = body.first() {
            if let Expr::StringLiteral(string) = expression.value.as_ref() {
                let docstring =
                    self.add_constant(Constant::String(clean_doc(string.value.to_str())))?;
                self.assembler
                    .set_location(self.source_location(expression.range));
                self.emit(LOAD_CONST, docstring, 1)?;
                self.store_name("__doc__")?;
                &body[1..]
            } else {
                body.as_slice()
            }
        } else {
            body.as_slice()
        };
        let body_start = self.assembler.instruction_count();
        let mut footer_location = SourceLocation::NONE;
        self.compile_suite(body)?;
        if let Some(Stmt::Expr(expression)) = body.last()
            && fold_constant(&expression.value).is_some()
        {
            let range = if let Expr::FString(fstring) = expression.value.as_ref() {
                self.fstring_result_range(fstring)
            } else {
                expression.range
            };
            self.assembler.set_location(self.source_location(range));
            self.emit(NOP, 0, 0)?;
        } else if let Some(Stmt::If(statement)) = body.last()
            && matches!(statement.test.as_ref(), Expr::EllipsisLiteral(_))
            && let Some(Stmt::Expr(expression)) = statement.body.last()
            && fold_constant(&expression.value).is_some()
        {
            // CPython transfers the final folded expression's location to the class footer.
            footer_location = self.source_location(expression.range);
        } else if self.assembler.instruction_count() > body_start
            && let Some(location) = self.assembler.last_instruction_location()
        {
            self.assembler.set_location(location);
        } else if let Some(statement) = body.last() {
            if !matches!(
                statement,
                Stmt::AnnAssign(annotation) if annotation.simple && annotation.value.is_none()
            ) {
                self.assembler
                    .set_location(self.source_location(statement.range()));
                self.emit(NOP, 0, 0)?;
            }
        }

        if self.unit.flags & CO_FUTURE_ANNOTATIONS == 0
            && let Some((annotation_child, closure_names)) = self.compile_class_annotations(body)?
        {
            let line = i32::try_from(self.unit.first_line_number).unwrap_or(i32::MAX);
            self.assembler
                .set_location(SourceLocation::new(line, line, 0, 0));
            self.emit_closure_tuple(&closure_names)?;
            let annotation = self.add_constant(Constant::Code(Box::new(annotation_child)))?;
            self.emit(LOAD_CONST, annotation, 1)?;
            self.emit(MAKE_FUNCTION, 0, 0)?;
            self.emit_function_attribute(FunctionAttribute::CLOSURE)?;
            self.store_name("__annotate_func__")?;
        }

        self.assembler.set_location(footer_location);
        let static_attributes = self.add_constant(Constant::Tuple(
            class_static_attributes(body)
                .into_iter()
                .map(Constant::String)
                .collect(),
        ))?;
        self.emit(LOAD_CONST, static_attributes, 1)?;
        self.store_name("__static_attributes__")?;
        if has_classdict_cell {
            let index = self
                .symbols
                .locals
                .iter()
                .position(|name| name == "__classdict__")
                .expect("classdict cell is present");
            self.emit(LOAD_FAST, to_u32(index, "classdict cell index")?, 1)?;
            self.store_name("__classdictcell__")?;
        }
        if has_class_cell {
            let index = self
                .symbols
                .locals
                .iter()
                .position(|name| name == "__class__")
                .expect("class cell is present");
            self.emit(LOAD_FAST, to_u32(index, "class cell index")?, 1)?;
            self.emit(COPY, 1, 1)?;
            self.store_name("__classcell__")?;
            self.emit(RETURN_VALUE, 0, -1)?;
            self.finish_inner(false)
        } else {
            self.finish()
        }
    }

    pub(super) fn finish(self) -> Result<CodeObject, CompileError> {
        self.finish_inner(true)
    }

    pub(super) fn finish_inner(
        mut self,
        add_implicit_return: bool,
    ) -> Result<CodeObject, CompileError> {
        for (instruction, constant) in
            std::mem::take(&mut self.output.deferred_constants_before_return)
        {
            let index = self.add_constant(constant)?;
            self.assembler.patch_argument(instruction, index);
        }
        if add_implicit_return {
            let none = self.add_constant(Constant::None)?;
            self.emit(LOAD_CONST, none, 1)?;
            self.emit(RETURN_VALUE, 0, -1)?;
        }
        if self.output.constants.is_empty()
            && !(self.unit.name == "<lambda>" && self.unit.flags & CO_GENERATOR != 0)
        {
            self.add_constant(Constant::None)?;
        }
        if let Some(start) = self.control.generator_region_start {
            let handler = self.assembler.label();
            self.assembler.mark(handler);
            let mut region_start = start;
            for (exclusion_start, exclusion_end) in &self.control.generator_region_exclusions {
                self.assembler.add_exception_region(
                    region_start,
                    *exclusion_start,
                    handler,
                    0,
                    true,
                );
                region_start = *exclusion_end;
            }
            self.assembler
                .add_exception_region(region_start, handler, handler, 0, true);
            self.assembler.set_location(SourceLocation::NONE);
            self.set_depth(1);
            self.emit_unary_intrinsic(UnaryIntrinsic::STOPITERATION_ERROR)?;
            self.emit(RERAISE, 1, -1)?;
        }

        if self.control.depth != 0 {
            return Err(CompileError::Internal(format!(
                "compiler finished with stack depth {}",
                self.control.depth
            )));
        }

        for (instruction, constant) in std::mem::take(&mut self.output.deferred_constants) {
            let index = self.add_constant(constant)?;
            if let Some(instruction) = instruction {
                self.assembler.patch_argument(instruction, index);
            }
        }
        for (instruction, name) in std::mem::take(&mut self.output.deferred_names) {
            let index = self.name_index(&name)?;
            self.assembler.patch_argument(instruction, index);
        }
        self.assembler.optimize_constant_pops();
        self.remove_unused_constants()?;

        let local_kinds = self
            .symbols
            .locals
            .iter()
            .enumerate()
            .map(|(index, name)| {
                let positional_only = usize::try_from(self.unit.positional_only_arg_count).unwrap();
                let positional = usize::try_from(self.unit.arg_count).unwrap();
                let keyword_only = usize::try_from(self.unit.keyword_only_arg_count).unwrap();
                let argument_kind = if index < positional_only {
                    CO_FAST_ARG_POS
                } else if index < positional {
                    CO_FAST_ARG_POS | CO_FAST_ARG_KW
                } else if index < positional + keyword_only {
                    CO_FAST_ARG_KW
                } else if self.unit.flags & CO_VARARGS != 0 && index == positional + keyword_only {
                    CO_FAST_ARG_POS | CO_FAST_ARG_VAR
                } else if self.unit.flags & CO_VARKEYWORDS != 0
                    && index
                        == positional
                            + keyword_only
                            + usize::from(self.unit.flags & CO_VARARGS != 0)
                {
                    CO_FAST_ARG_KW | CO_FAST_ARG_VAR
                } else {
                    0
                };
                let storage_kind =
                    if index >= self.symbols.locals.len() - self.symbols.free_names.len() {
                        CO_FAST_FREE
                    } else if self.is_cell(name) {
                        if index < self.symbols.fast_local_count {
                            CO_FAST_LOCAL | CO_FAST_CELL
                        } else {
                            CO_FAST_CELL
                        }
                    } else {
                        CO_FAST_LOCAL
                    };
                argument_kind
                    | storage_kind
                    | if self.is_hidden(name) {
                        CO_FAST_HIDDEN
                    } else {
                        0
                    }
            })
            .collect();
        let output_locals = self
            .symbols
            .locals
            .iter()
            .map(|name| self.mangled_name(name))
            .collect();
        let parameter_count = usize::try_from(
            self.unit.arg_count
                + self.unit.keyword_only_arg_count
                + u32::from(self.unit.flags & CO_VARARGS != 0)
                + u32::from(self.unit.flags & CO_VARKEYWORDS != 0),
        )
        .unwrap_or(usize::MAX);
        let AssembledCode {
            bytecode,
            line_table,
            exception_table,
            max_depth: assembled_max_depth,
            removed_max_depth,
        } = self.assembler.finish_code(
            self.unit.first_line_number,
            self.symbols.fast_local_count,
            parameter_count,
        )?;
        let max_depth = if removed_max_depth == Some(self.control.max_depth)
            && assembled_max_depth < self.control.max_depth
        {
            assembled_max_depth
        } else {
            self.control.max_depth
        };
        let stack_size = if self.unit.flags & (CO_GENERATOR | CO_COROUTINE | CO_ASYNC_GENERATOR)
            != 0
            && self.control.generator_region_start.is_some()
        {
            max_depth.max(2)
        } else {
            max_depth.max(1)
        };
        Ok(CodeObject {
            arg_count: self.unit.arg_count,
            positional_only_arg_count: self.unit.positional_only_arg_count,
            keyword_only_arg_count: self.unit.keyword_only_arg_count,
            stack_size,
            flags: self.unit.flags,
            bytecode,
            constants: self.output.constants,
            names: self.output.names,
            locals: output_locals,
            local_kinds,
            filename: self.context.filename.clone(),
            name: self.unit.name,
            qualified_name: self.unit.qualified_name,
            first_line_number: self.unit.first_line_number,
            line_table,
            exception_table,
            annotation_thunk: self.unit.annotation_thunk,
            interned_constant_strings: self.output.interned_constant_strings,
        })
    }

    pub(super) fn emit_function_prologue(&mut self) -> Result<(), CompileError> {
        self.assembler.set_location(SourceLocation::NONE);
        if !self.symbols.free_names.is_empty() {
            self.emit(
                COPY_FREE_VARS,
                to_u32(self.symbols.free_names.len(), "free variable count")?,
                0,
            )?;
        }
        let cells: Vec<_> = self
            .symbols
            .locals
            .iter()
            .take(self.symbols.locals.len() - self.symbols.free_names.len())
            .enumerate()
            .filter(|(_, name)| self.is_cell(name))
            .map(|(index, _)| index)
            .collect();
        for index in cells {
            self.emit(MAKE_CELL, to_u32(index, "cell variable index")?, 0)?;
        }
        if self.unit.flags & (CO_GENERATOR | CO_COROUTINE | CO_ASYNC_GENERATOR) != 0 {
            let line = i32::try_from(self.unit.first_line_number).unwrap_or(i32::MAX);
            self.assembler
                .set_location(SourceLocation::new(line, line, -1, -1));
            self.assembler.emit(RETURN_GENERATOR, 0);
            self.assembler.emit(POP_TOP, 0);
            let start = self.assembler.label();
            self.assembler.mark(start);
            self.control.generator_region_start = Some(start);
        }
        let line = i32::try_from(self.unit.first_line_number).unwrap_or(i32::MAX);
        self.assembler
            .set_location(SourceLocation::new(line, line, 0, 0));
        self.emit_resume(ResumeLocation::AT_FUNC_START, false)
    }
}

impl CodeUnit {
    fn from_spec(spec: CodeUnitSpec) -> Self {
        Self {
            scope: spec.scope,
            scope_id: spec.scope_id,
            generic_target_qualified_name: None,
            child_qualified_name_parent: None,
            class_scope_is_nested: false,
            module_annotation_index: 0,
            annotation_classdict_index: None,
            name: spec.name,
            qualified_name: spec.qualified_name,
            private_name: spec.private_name,
            arg_count: spec.arg_count,
            positional_only_arg_count: spec.positional_only_arg_count,
            keyword_only_arg_count: spec.keyword_only_arg_count,
            flags: spec.flags,
            first_line_number: spec.first_line_number,
            annotation_thunk: false,
        }
    }
}
