// Compiler submodules intentionally share the parent-owned state and lowering vocabulary.
#![allow(clippy::wildcard_imports)]

use super::*;

impl Compiler {
    pub(super) fn compile_type_alias(
        &mut self,
        statement: &ruff_python_ast::StmtTypeAlias,
    ) -> Result<(), CompileError> {
        let Expr::Name(name) = statement.name.as_ref() else {
            return Err(CompileError::Internal(
                "type alias name is not an identifier".to_string(),
            ));
        };
        if let Some(type_params) = statement.type_params.as_deref() {
            return self.compile_generic_type_alias(
                name.id.as_str(),
                &statement.value,
                type_params,
                statement,
            );
        }

        self.assembler
            .set_location(self.source_location(statement.range));
        self.load_string_constant(name.id.as_str())?;
        let none = self.add_constant(Constant::None)?;
        self.emit(LOAD_CONST, none, 1)?;
        self.compile_type_parameter_thunk(
            name.id.as_str(),
            &statement.value,
            statement.range,
            self.child_function_is_nested(),
        )?;
        self.emit_build(BUILD_TUPLE, 3)?;
        self.emit_unary_intrinsic(UnaryIntrinsic::TYPEALIAS)?;
        self.store_name(name.id.as_str())
    }

    fn compile_generic_type_alias(
        &mut self,
        name: &str,
        value: &Expr,
        type_params: &ruff_python_ast::TypeParams,
        statement: &ruff_python_ast::StmtTypeAlias,
    ) -> Result<(), CompileError> {
        let type_names: BTreeSet<_> = type_params
            .iter()
            .map(|parameter| parameter.name().as_str().to_string())
            .collect();
        let locals = type_params
            .iter()
            .map(|parameter| parameter.name().as_str().to_string())
            .collect();
        let mut requirements = type_parameter_required_names(type_params);
        requirements.extend(expression_required_names(value));
        let cellvars = requirements.intersection(&type_names).cloned().collect();
        let mut freevars: BTreeSet<_> = requirements
            .iter()
            .filter(|name| !type_names.contains(*name))
            .filter(|name| self.can_provide_closure(name))
            .cloned()
            .collect();
        if matches!(self.unit.scope, Scope::Class { .. }) {
            freevars.insert("__classdict__".to_string());
        }
        let plan = LoweringFunctionPlan {
            locals,
            cellvars,
            freevars,
            ..LoweringFunctionPlan::default()
        };
        let wrapper_name = format!("<generic parameters of {name}>");
        let wrapper_flags = if self.child_function_is_nested() {
            CO_NESTED
        } else {
            0
        } | (self.unit.flags & SUPPORTED_FUTURE_FLAGS);
        let mut wrapper = Self::function(
            Arc::clone(&self.context),
            &wrapper_name,
            self.child_qualified_name(&wrapper_name),
            self.line_number(u32::from(statement.range.start())),
            &plan,
            self.unit.scope_id,
            0,
            0,
            0,
            wrapper_flags,
        )?;
        wrapper
            .unit
            .private_name
            .clone_from(&self.unit.private_name);
        wrapper
            .symbols
            .imported_scope_names
            .clone_from(&self.symbols.imported_scope_names);
        let type_parameter_names = type_names
            .iter()
            .map(|name| wrapper.required_name_id(name))
            .collect();
        wrapper.symbols.type_parameter_names = type_parameter_names;
        wrapper.unit.generic_target_qualified_name = Some(self.child_qualified_name(name));
        if matches!(self.unit.scope, Scope::Class { .. }) {
            wrapper.unit.annotation_classdict_index = Some(wrapper.closure_index("__classdict__")?);
        }
        wrapper.emit_function_prologue()?;
        wrapper
            .assembler
            .set_location(wrapper.source_location(statement.range));
        wrapper.load_string_constant(name)?;
        wrapper.compile_type_parameters(type_params)?;
        wrapper.compile_type_parameter_thunk(name, value, statement.range, true)?;
        wrapper.emit_build(BUILD_TUPLE, 3)?;
        wrapper.emit_unary_intrinsic(UnaryIntrinsic::TYPEALIAS)?;
        wrapper.emit(RETURN_VALUE, 0, -1)?;
        let wrapper = wrapper.finish_inner(false)?;

        let wrapper_closure_names: Vec<_> = wrapper
            .locals
            .iter()
            .zip(&wrapper.local_kinds)
            .filter(|(_, kind)| **kind & CO_FAST_FREE != 0)
            .map(|(name, _)| name.clone())
            .collect();
        if !wrapper_closure_names.is_empty() {
            self.emit_closure_tuple(&wrapper_closure_names)?;
        }
        let code = self.add_constant(Constant::Code(Box::new(wrapper)))?;
        self.emit(LOAD_CONST, code, 1)?;
        self.emit(MAKE_FUNCTION, 0, 0)?;
        if !wrapper_closure_names.is_empty() {
            self.emit_function_attribute(FunctionAttribute::CLOSURE)?;
        }
        self.emit(PUSH_NULL, 0, 1)?;
        self.emit(CALL, 0, -1)?;
        self.store_name(name)
    }

    pub(super) fn compile_function_definition(
        &mut self,
        definition: &StmtFunctionDef,
    ) -> Result<(), CompileError> {
        if let Some(type_params) = definition.type_params.as_deref() {
            return self.compile_generic_function_definition(definition, type_params);
        }
        self.compile_plain_function_definition(definition, None, DefinitionDecorators::Compile)
    }

    fn compile_generic_function_definition(
        &mut self,
        definition: &StmtFunctionDef,
        type_params: &ruff_python_ast::TypeParams,
    ) -> Result<(), CompileError> {
        for decorator in &definition.decorator_list {
            self.compile_expression(&decorator.expression)?;
        }
        let definition_location = self.definition_location(
            definition.range,
            definition.name.range(),
            b"def",
            definition.is_async,
        );
        let defaults = self.compile_function_defaults(definition, definition_location)?;
        let default_argument_count = usize::from(defaults.0) + usize::from(defaults.1);
        if default_argument_count == 2 {
            self.assembler.set_location(definition_location);
            self.emit(SWAP, 2, 0)?;
        }
        let type_names: BTreeSet<_> = type_params
            .iter()
            .map(|parameter| parameter.name().as_str().to_string())
            .collect();
        let child_scope_id = self.function_scope_id(definition)?;
        let arena = self.scope_arena()?;
        let mut child_plan = arena
            .function(child_scope_id)
            .ok_or_else(|| CompileError::Internal("function scope has no plan".to_string()))?
            .clone();
        for name in child_plan.required.iter().copied().collect::<Vec<_>>() {
            let spelling = arena.names.resolve(name);
            if type_names.contains(spelling) || self.can_provide_closure(spelling) {
                child_plan.insert_freevar(&arena.names, name);
            } else {
                child_plan.mark_global(name);
            }
        }
        let child_plan = child_plan.materialize(&arena.names);

        let mut locals = vec![".defaults".to_string()];
        if defaults.1 {
            locals.push(".kwdefaults".to_string());
        }
        locals.extend(
            type_params
                .iter()
                .map(|parameter| parameter.name().as_str().to_string()),
        );
        let type_parameter_requirements = type_parameter_required_names(type_params);
        let wrapper_local_names: HashSet<_> = locals.iter().cloned().collect();
        let mut wrapper_plan = LoweringFunctionPlan {
            locals,
            cellvars: type_parameter_requirements
                .intersection(&type_names)
                .cloned()
                .collect(),
            required: type_parameter_requirements
                .iter()
                .filter(|name| !wrapper_local_names.contains(*name))
                .cloned()
                .collect(),
            ..LoweringFunctionPlan::default()
        };
        wrapper_plan.absorb_child_requirements(
            child_plan.required.iter().cloned(),
            child_plan.annotation_references.iter().cloned(),
        );
        for name in wrapper_plan.required.iter().cloned().collect::<Vec<_>>() {
            if self.can_provide_closure(&name) {
                wrapper_plan.freevars.insert(name);
            } else {
                wrapper_plan.mark_global(&name);
            }
        }
        if matches!(self.unit.scope, Scope::Class { .. }) {
            wrapper_plan.freevars.insert("__classdict__".to_string());
        }
        let wrapper_name = format!("<generic parameters of {}>", definition.name);
        let target_qualified_name = self.child_qualified_name(definition.name.as_str());
        let wrapper_flags = if self.child_function_is_nested() {
            CO_NESTED
        } else {
            0
        } | (self.unit.flags & SUPPORTED_FUTURE_FLAGS);
        let mut wrapper = Self::function(
            Arc::clone(&self.context),
            &wrapper_name,
            self.child_qualified_name(&wrapper_name),
            self.line_number(u32::from(
                definition
                    .decorator_list
                    .first()
                    .map_or(definition.range.start(), |decorator| {
                        decorator.expression.range().start()
                    }),
            )),
            &wrapper_plan,
            self.unit.scope_id,
            to_u32(
                default_argument_count,
                "generic function default argument count",
            )?,
            0,
            0,
            wrapper_flags,
        )?;
        wrapper
            .unit
            .private_name
            .clone_from(&self.unit.private_name);
        wrapper
            .symbols
            .imported_scope_names
            .clone_from(&self.symbols.imported_scope_names);
        let type_parameter_names = type_names
            .iter()
            .map(|name| wrapper.required_name_id(name))
            .collect();
        wrapper.symbols.type_parameter_names = type_parameter_names;
        wrapper.unit.generic_target_qualified_name = Some(target_qualified_name);
        if matches!(self.unit.scope, Scope::Class { .. }) {
            wrapper.unit.annotation_classdict_index = Some(wrapper.closure_index("__classdict__")?);
        }
        wrapper.emit_function_prologue()?;
        wrapper.compile_type_parameters(type_params)?;
        wrapper.assembler.set_location(definition_location);
        for index in 0..default_argument_count {
            wrapper.emit(
                LOAD_FAST,
                to_u32(index, "generic function default argument index")?,
                1,
            )?;
        }
        if default_argument_count > 0 {
            // CPython inserts closure loads after fusing the synthetic default
            // argument loads, so the last default does not fuse with a cell.
            wrapper.assembler.prevent_last_instruction_fusion();
        }
        wrapper.compile_plain_function_definition(
            definition,
            Some(defaults),
            DefinitionDecorators::AlreadyCompiled,
        )?;
        wrapper.emit(SWAP, 2, 0)?;
        wrapper.emit_binary_intrinsic(BinaryIntrinsic::SET_FUNCTION_TYPE_PARAMS)?;
        wrapper.emit(RETURN_VALUE, 0, -1)?;
        let wrapper = wrapper.finish_inner(false)?;

        let wrapper_closure_names: Vec<_> = wrapper
            .locals
            .iter()
            .zip(&wrapper.local_kinds)
            .filter(|(_, kind)| **kind & CO_FAST_FREE != 0)
            .map(|(name, _)| name.clone())
            .collect();
        if !wrapper_closure_names.is_empty() {
            self.emit_closure_tuple(&wrapper_closure_names)?;
        }
        self.assembler.set_location(definition_location);
        let code = self.add_constant(Constant::Code(Box::new(wrapper)))?;
        self.emit(LOAD_CONST, code, 1)?;
        self.emit(MAKE_FUNCTION, 0, 0)?;
        if !wrapper_closure_names.is_empty() {
            self.emit_function_attribute(FunctionAttribute::CLOSURE)?;
        }
        if default_argument_count == 0 {
            self.emit(PUSH_NULL, 0, 1)?;
            self.emit(CALL, 0, -1)?;
        } else {
            self.emit(
                SWAP,
                to_u32(
                    default_argument_count + 1,
                    "generic function wrapper call depth",
                )?,
                0,
            )?;
            self.emit(
                CALL,
                to_u32(
                    default_argument_count - 1,
                    "generic function wrapper call argument count",
                )?,
                -i32::try_from(default_argument_count).unwrap(),
            )?;
        }
        for decorator in definition.decorator_list.iter().rev() {
            self.assembler
                .set_location(self.source_location(decorator.expression.range()));
            self.emit(CALL, 0, -1)?;
        }
        self.assembler.set_location(definition_location);
        self.store_name(definition.name.as_str())
    }

    fn compile_plain_function_definition(
        &mut self,
        definition: &StmtFunctionDef,
        preloaded_defaults: Option<(bool, bool)>,
        decorators: DefinitionDecorators,
    ) -> Result<(), CompileError> {
        let parameters = &definition.parameters;
        if decorators == DefinitionDecorators::Compile {
            for decorator in &definition.decorator_list {
                self.compile_expression(&decorator.expression)?;
            }
        }
        let definition_location = self.definition_location(
            definition.range,
            definition.name.range(),
            b"def",
            definition.is_async,
        );

        let (has_positional_defaults, has_keyword_defaults) =
            if let Some(defaults) = preloaded_defaults {
                defaults
            } else {
                self.compile_function_defaults(definition, definition_location)?
            };

        let (scope_id, plan) = self.resolved_function_plan(definition)?;
        let closure_names: Vec<_> = plan.freevars.iter().cloned().collect();
        let annotation_child = self.compile_function_annotations(definition, &plan)?;

        let arg_count = to_u32(
            parameters.posonlyargs.len() + parameters.args.len(),
            "function argument count",
        )?;
        let positional_only_arg_count = to_u32(
            parameters.posonlyargs.len(),
            "positional-only argument count",
        )?;
        let keyword_only_arg_count =
            to_u32(parameters.kwonlyargs.len(), "keyword-only argument count")?;
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
        } else {
            0
        }) | (self.unit.flags & SUPPORTED_FUTURE_FLAGS);
        if definition.is_async && suite_contains_yield(&definition.body) {
            parameter_flags |= CO_ASYNC_GENERATOR;
        } else if definition.is_async {
            parameter_flags |= CO_COROUTINE;
        } else if suite_contains_yield(&definition.body) {
            parameter_flags |= CO_GENERATOR;
        }
        let qualified_name = self
            .unit
            .generic_target_qualified_name
            .clone()
            .unwrap_or_else(|| self.child_qualified_name(definition.name.as_str()));
        let first_line_number =
            self.line_number(u32::from(if decorators == DefinitionDecorators::Compile {
                definition
                    .decorator_list
                    .first()
                    .map_or(definition.range.start(), |decorator| {
                        decorator.expression.range().start()
                    })
            } else {
                definition.range.start()
            }));
        let mut child = Self::function(
            Arc::clone(&self.context),
            definition.name.as_str(),
            qualified_name,
            first_line_number,
            &plan,
            scope_id,
            arg_count,
            positional_only_arg_count,
            keyword_only_arg_count,
            parameter_flags,
        )?;
        child.unit.private_name.clone_from(&self.unit.private_name);
        child
            .symbols
            .imported_scope_names
            .clone_from(&self.symbols.imported_scope_names);
        let child = child.compile_function_body(&definition.body)?;

        self.assembler.set_location(definition_location);
        if let Some(annotation_child) = annotation_child {
            let annotation_closure_names: Vec<_> = annotation_child
                .locals
                .iter()
                .zip(&annotation_child.local_kinds)
                .filter(|(_, kind)| **kind & CO_FAST_FREE != 0)
                .map(|(name, _)| name.clone())
                .collect();
            if !annotation_closure_names.is_empty() {
                self.emit_closure_tuple(&annotation_closure_names)?;
            }
            let annotation = self.add_constant(Constant::Code(Box::new(annotation_child)))?;
            self.emit(LOAD_CONST, annotation, 1)?;
            self.emit(MAKE_FUNCTION, 0, 0)?;
            if !annotation_closure_names.is_empty() {
                self.emit_function_attribute(FunctionAttribute::CLOSURE)?;
            }
        }
        if !closure_names.is_empty() {
            self.emit_closure_tuple(&closure_names)?;
        }
        let constant = self.add_constant(Constant::Code(Box::new(child)))?;
        self.emit(LOAD_CONST, constant, 1)?;
        self.emit(MAKE_FUNCTION, 0, 0)?;
        if !closure_names.is_empty() {
            self.emit_function_attribute(FunctionAttribute::CLOSURE)?;
        }
        if function_has_annotations(definition) {
            self.emit_function_attribute(FunctionAttribute::ANNOTATE)?;
        }
        if has_keyword_defaults {
            self.emit_function_attribute(FunctionAttribute::KWDEFAULTS)?;
        }
        if has_positional_defaults {
            self.emit_function_attribute(FunctionAttribute::DEFAULTS)?;
        }
        if decorators == DefinitionDecorators::Compile {
            for decorator in definition.decorator_list.iter().rev() {
                self.assembler
                    .set_location(self.source_location(decorator.expression.range()));
                self.emit(CALL, 0, -1)?;
            }
        }
        self.assembler.set_location(definition_location);
        if self.unit.generic_target_qualified_name.is_some() {
            Ok(())
        } else {
            self.store_name(definition.name.as_str())
        }
    }

    fn compile_function_defaults(
        &mut self,
        definition: &StmtFunctionDef,
        definition_location: SourceLocation,
    ) -> Result<(bool, bool), CompileError> {
        let parameters = &definition.parameters;
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
                self.assembler.set_location(definition_location);
                self.emit_deferred_constant(Constant::Tuple(defaults))?;
            } else {
                for default in &positional_defaults {
                    self.compile_expression(default)?;
                }
                self.assembler.set_location(definition_location);
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
                self.assembler.set_location(definition_location);
                let name = self.add_constant(Constant::String(self.mangled_name(name)))?;
                self.emit(LOAD_CONST, name, 1)?;
                self.compile_expression(default)?;
            }
            let count = to_u32(keyword_defaults.len(), "keyword default count")?;
            self.assembler.set_location(definition_location);
            self.emit(BUILD_MAP, count, 1 - i32::try_from(count).unwrap() * 2)?;
        }

        Ok((
            !positional_defaults.is_empty(),
            !keyword_defaults.is_empty(),
        ))
    }

    pub(super) fn compile_class_definition(
        &mut self,
        definition: &StmtClassDef,
    ) -> Result<(), CompileError> {
        if let Some(type_params) = definition.type_params.as_deref() {
            return self.compile_generic_class_definition(definition, type_params);
        }
        self.compile_plain_class_definition(definition, DefinitionDecorators::Compile)
    }

    fn compile_generic_class_definition(
        &mut self,
        definition: &StmtClassDef,
        type_params: &ruff_python_ast::TypeParams,
    ) -> Result<(), CompileError> {
        for decorator in &definition.decorator_list {
            self.compile_expression(&decorator.expression)?;
        }
        let type_names: BTreeSet<_> = type_params
            .iter()
            .map(|parameter| parameter.name().as_str().to_string())
            .collect();
        let mut locals: Vec<_> = type_params
            .iter()
            .map(|parameter| parameter.name().as_str().to_string())
            .collect();
        locals.push(".generic_base".to_string());
        locals.push(".type_params".to_string());
        let (_, class_plan) = self.class_plan(definition)?;
        let required_names = &class_plan.required;
        let type_parameter_requirements = type_parameter_required_names(type_params);
        let mut type_parameter_cell_requirements = type_parameter_requirements.clone();
        let mut wrapper_requirements = type_parameter_requirements.clone();
        if let Some(arguments) = definition.arguments.as_deref() {
            for argument in &arguments.args {
                wrapper_requirements.extend(expression_required_names(argument));
                type_parameter_cell_requirements.extend(nested_expression_required_names(argument));
            }
            for keyword in &arguments.keywords {
                wrapper_requirements.extend(expression_required_names(&keyword.value));
                type_parameter_cell_requirements
                    .extend(nested_expression_required_names(&keyword.value));
            }
        }
        let mut cellvars: HashSet<_> = type_names
            .intersection(required_names)
            .chain(type_names.intersection(&type_parameter_cell_requirements))
            .cloned()
            .collect();
        cellvars.insert(".type_params".to_string());
        let mut freevars: BTreeSet<_> = required_names
            .iter()
            .chain(&wrapper_requirements)
            .filter(|name| !type_names.contains(*name))
            .filter(|name| self.can_provide_closure(name))
            .cloned()
            .collect();
        // A type-parameter scope nested directly in a class can resolve names through the
        // class namespace. CPython provides that namespace through the synthetic
        // `__classdict__` closure, even when no type-parameter expression uses a class name.
        if matches!(self.unit.scope, Scope::Class { .. }) {
            freevars.insert("__classdict__".to_string());
        }
        let plan = LoweringFunctionPlan {
            locals,
            cellvars,
            freevars,
            ..LoweringFunctionPlan::default()
        };
        let wrapper_name = format!("<generic parameters of {}>", definition.name);
        let target_qualified_name = self.child_qualified_name(definition.name.as_str());
        let wrapper_child_qualified_name_parent = target_qualified_name
            .rsplit_once('.')
            .map_or("", |(parent, _)| parent)
            .to_string();
        let wrapper_flags = if self.child_function_is_nested() {
            CO_NESTED
        } else {
            0
        } | (self.unit.flags & SUPPORTED_FUTURE_FLAGS);
        let mut wrapper = Self::function(
            Arc::clone(&self.context),
            &wrapper_name,
            self.child_qualified_name(&wrapper_name),
            self.line_number(u32::from(
                definition
                    .decorator_list
                    .first()
                    .map_or(definition.range.start(), |decorator| {
                        decorator.expression.range().start()
                    }),
            )),
            &plan,
            self.unit.scope_id,
            0,
            0,
            0,
            wrapper_flags,
        )?;
        wrapper
            .unit
            .private_name
            .clone_from(&self.unit.private_name);
        wrapper
            .symbols
            .imported_scope_names
            .clone_from(&self.symbols.imported_scope_names);
        let type_parameter_names = type_names
            .iter()
            .map(|name| wrapper.required_name_id(name))
            .collect();
        wrapper.symbols.type_parameter_names = type_parameter_names;
        wrapper.unit.generic_target_qualified_name = Some(target_qualified_name);
        wrapper.unit.child_qualified_name_parent = Some(wrapper_child_qualified_name_parent);
        if matches!(self.unit.scope, Scope::Class { .. }) {
            wrapper.unit.annotation_classdict_index = Some(wrapper.closure_index("__classdict__")?);
        }
        wrapper.emit_function_prologue()?;
        wrapper.compile_type_parameters(type_params)?;
        wrapper.assembler.set_location(wrapper.definition_location(
            definition.range,
            definition.name.range(),
            b"class",
            false,
        ));
        wrapper.store_name(".type_params")?;
        wrapper
            .compile_plain_class_definition(definition, DefinitionDecorators::AlreadyCompiled)?;
        wrapper.emit(RETURN_VALUE, 0, -1)?;
        let wrapper = wrapper.finish_inner(false)?;
        let wrapper_closure_names: Vec<_> = wrapper
            .locals
            .iter()
            .zip(&wrapper.local_kinds)
            .filter(|(_, kind)| **kind & CO_FAST_FREE != 0)
            .map(|(name, _)| name.clone())
            .collect();

        let definition_location =
            self.definition_location(definition.range, definition.name.range(), b"class", false);
        self.assembler.set_location(definition_location);
        if !wrapper_closure_names.is_empty() {
            self.emit_closure_tuple(&wrapper_closure_names)?;
        }
        let code = self.add_constant(Constant::Code(Box::new(wrapper)))?;
        self.emit(LOAD_CONST, code, 1)?;
        self.emit(MAKE_FUNCTION, 0, 0)?;
        if !wrapper_closure_names.is_empty() {
            self.emit_function_attribute(FunctionAttribute::CLOSURE)?;
        }
        self.emit(PUSH_NULL, 0, 1)?;
        self.emit(CALL, 0, -1)?;
        for decorator in definition.decorator_list.iter().rev() {
            self.assembler
                .set_location(self.source_location(decorator.expression.range()));
            self.emit(CALL, 0, -1)?;
        }
        self.assembler.set_location(definition_location);
        self.store_name(definition.name.as_str())
    }

    fn compile_plain_class_definition(
        &mut self,
        definition: &StmtClassDef,
        decorators: DefinitionDecorators,
    ) -> Result<(), CompileError> {
        if decorators == DefinitionDecorators::Compile {
            for decorator in &definition.decorator_list {
                self.compile_expression(&decorator.expression)?;
            }
        }
        let is_generic = self.unit.generic_target_qualified_name.is_some();

        let (scope_id, class_plan) = self.class_plan(definition)?;
        let mut freevars = class_plan
            .required
            .iter()
            .filter(|name| match &self.unit.scope {
                Scope::Module => false,
                Scope::Class { free_indices, .. } => {
                    self.is_cell(name) || self.scope_index(free_indices, name).is_some()
                }
                Scope::Function {
                    indices,
                    free_indices,
                    cells,
                    ..
                } => {
                    self.scope_index(indices, name).is_some() && self.contains_name(cells, name)
                        || self.scope_index(free_indices, name).is_some()
                }
            })
            .cloned()
            .collect::<BTreeSet<_>>();
        if is_generic {
            freevars.insert(".type_params".to_string());
        }
        let closure_names = freevars.iter().cloned().collect::<Vec<_>>();
        let qualified_name = self
            .unit
            .generic_target_qualified_name
            .clone()
            .unwrap_or_else(|| self.child_qualified_name(definition.name.as_str()));
        let first_line_number =
            self.line_number(u32::from(if decorators == DefinitionDecorators::Compile {
                definition
                    .decorator_list
                    .first()
                    .map_or(definition.range.start(), |decorator| {
                        decorator.expression.range().start()
                    })
            } else {
                definition.range.start()
            }));
        let class_scope_is_nested = self.child_function_is_nested();
        let mut child = Self::class(
            Arc::clone(&self.context),
            definition.name.as_str(),
            qualified_name,
            first_line_number,
            scope_id,
            class_plan.globals,
            class_plan.nonlocals,
            freevars,
            class_plan.bound_names,
            class_plan.needs_class_closure,
            class_plan.needs_classdict,
            self.unit.flags & SUPPORTED_FUTURE_FLAGS,
        );
        child.unit.class_scope_is_nested = class_scope_is_nested;
        child
            .symbols
            .type_parameter_names
            .clone_from(&self.symbols.type_parameter_names);
        child
            .symbols
            .imported_scope_names
            .clone_from(&self.symbols.imported_scope_names);
        let child = child.compile_class_body(&definition.body)?;

        let definition_location =
            self.definition_location(definition.range, definition.name.range(), b"class", false);
        self.assembler.set_location(definition_location);
        self.emit(LOAD_BUILD_CLASS, 0, 1)?;
        self.emit(PUSH_NULL, 0, 1)?;
        if !closure_names.is_empty() {
            self.emit_closure_tuple(&closure_names)?;
        }
        let code = self.add_constant(Constant::Code(Box::new(child)))?;
        self.emit(LOAD_CONST, code, 1)?;
        self.emit(MAKE_FUNCTION, 0, 0)?;
        if !closure_names.is_empty() {
            self.emit_function_attribute(FunctionAttribute::CLOSURE)?;
        }
        let name = self.add_constant(Constant::String(definition.name.as_str().to_string()))?;
        self.emit(LOAD_CONST, name, 1)?;
        if is_generic {
            self.load_name(".type_params")?;
            self.emit_unary_intrinsic(UnaryIntrinsic::SUBSCRIPT_GENERIC)?;
            self.store_name(".generic_base")?;
        }

        let arguments = definition.arguments.as_deref();
        let has_unpacking = arguments.is_some_and(|arguments| {
            arguments.args.iter().any(Expr::is_starred_expr)
                || arguments
                    .keywords
                    .iter()
                    .any(|keyword| keyword.arg.is_none())
        });
        let has_starred_arguments =
            arguments.is_some_and(|arguments| arguments.args.iter().any(Expr::is_starred_expr));
        let argument_count = arguments.map_or(0, |arguments| {
            arguments.args.len() + arguments.keywords.len()
        }) + usize::from(is_generic);
        if has_unpacking {
            // Extended class calls include the body function and class name in their positional
            // tuple. CPython packs those together with every base before the first starred base.
            if has_starred_arguments {
                let leading_count = arguments.map_or(0, |arguments| {
                    arguments
                        .args
                        .iter()
                        .take_while(|argument| !argument.is_starred_expr())
                        .count()
                });
                if let Some(arguments) = arguments {
                    for argument in arguments.args.iter().take(leading_count) {
                        self.compile_expression(argument)?;
                    }
                }
                self.emit_build(BUILD_LIST, leading_count + 2)?;
                for argument in arguments
                    .expect("starred class arguments require an argument list")
                    .args
                    .iter()
                    .skip(leading_count)
                {
                    if let Expr::Starred(starred) = argument {
                        self.compile_expression(&starred.value)?;
                        self.emit(LIST_EXTEND, 1, -1)?;
                    } else {
                        self.compile_expression(argument)?;
                        self.emit(LIST_APPEND, 1, -1)?;
                    }
                }
                if is_generic {
                    self.load_name(".generic_base")?;
                    self.emit(LIST_APPEND, 1, -1)?;
                }
                self.emit_unary_intrinsic(UnaryIntrinsic::LIST_TO_TUPLE)?;
            } else {
                if let Some(arguments) = arguments {
                    for argument in &arguments.args {
                        self.compile_expression(argument)?;
                    }
                }
                if is_generic {
                    self.load_name(".generic_base")?;
                }
                let positional_count =
                    arguments.map_or(0, |arguments| arguments.args.len()) + usize::from(is_generic);
                self.emit_build(BUILD_TUPLE, positional_count + 2)?;
            }
            if let Some(arguments) = arguments.filter(|arguments| !arguments.keywords.is_empty()) {
                let mut have_dict = false;
                let mut named_start = 0;
                for (index, keyword) in arguments.keywords.iter().enumerate() {
                    if keyword.arg.is_some() {
                        continue;
                    }
                    if named_start < index {
                        self.compile_class_keyword_map(
                            &arguments.keywords[named_start..index],
                            definition_location,
                        )?;
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
                    self.emit(DICT_MERGE, 1, -1)?;
                    named_start = index + 1;
                }
                if named_start < arguments.keywords.len() {
                    self.compile_class_keyword_map(
                        &arguments.keywords[named_start..],
                        definition_location,
                    )?;
                    if have_dict {
                        self.emit(DICT_MERGE, 1, -1)?;
                    }
                }
            } else {
                self.emit(PUSH_NULL, 0, 1)?;
            }
            self.assembler.set_location(definition_location);
            self.emit(CALL_FUNCTION_EX, 0, -3)?;
        } else {
            if let Some(arguments) = arguments {
                for argument in &arguments.args {
                    self.compile_expression(argument)?;
                }
            }
            if is_generic {
                self.load_name(".generic_base")?;
            }
            if let Some(arguments) = arguments {
                for keyword in &arguments.keywords {
                    self.compile_expression(&keyword.value)?;
                }
            }
            self.assembler.set_location(definition_location);
            if let Some(arguments) = arguments.filter(|arguments| !arguments.keywords.is_empty()) {
                let keyword_names = arguments
                    .keywords
                    .iter()
                    .map(|keyword| {
                        Constant::String(keyword.arg.as_ref().unwrap().as_str().to_string())
                    })
                    .collect();
                let names = self.add_interned_string_tuple(keyword_names)?;
                self.emit(LOAD_CONST, names, 1)?;
                let count = to_u32(argument_count + 2, "class argument count")?;
                self.emit(CALL_KW, count, -i32::try_from(argument_count).unwrap() - 4)?;
            } else {
                let count = to_u32(argument_count + 2, "class argument count")?;
                self.emit(CALL, count, -i32::try_from(argument_count).unwrap() - 3)?;
            }
        }
        if decorators == DefinitionDecorators::Compile {
            for decorator in definition.decorator_list.iter().rev() {
                self.assembler
                    .set_location(self.source_location(decorator.expression.range()));
                self.emit(CALL, 0, -1)?;
            }
        }
        self.assembler.set_location(definition_location);
        if self.unit.generic_target_qualified_name.is_some() {
            Ok(())
        } else {
            self.store_name(definition.name.as_str())
        }
    }

    fn compile_class_keyword_map(
        &mut self,
        keywords: &[Keyword],
        definition_location: SourceLocation,
    ) -> Result<(), CompileError> {
        debug_assert!(!keywords.is_empty() && keywords.iter().all(|keyword| keyword.arg.is_some()));
        for keyword in keywords {
            let name = keyword.arg.as_ref().unwrap();
            let key = self.add_constant(Constant::String(name.as_str().to_string()))?;
            self.emit(LOAD_CONST, key, 1)?;
            self.compile_expression(&keyword.value)?;
        }
        self.assembler.set_location(definition_location);
        self.emit_build_map(keywords.len())
    }

    fn compile_type_parameters(
        &mut self,
        type_params: &ruff_python_ast::TypeParams,
    ) -> Result<(), CompileError> {
        for parameter in type_params {
            let name = parameter.name().as_str();
            self.assembler
                .set_location(self.source_location(parameter.range()));
            self.load_string_constant(name)?;
            match parameter {
                TypeParam::TypeVar(parameter) => {
                    if let Some(bound) = &parameter.bound {
                        self.compile_type_parameter_thunk(name, bound, bound.range(), true)?;
                        self.assembler
                            .set_location(self.source_location(parameter.range()));
                        let intrinsic = if matches!(bound.as_ref(), Expr::Tuple(_)) {
                            BinaryIntrinsic::TYPEVAR_WITH_CONSTRAINTS
                        } else {
                            BinaryIntrinsic::TYPEVAR_WITH_BOUND
                        };
                        self.emit_binary_intrinsic(intrinsic)?;
                    } else {
                        self.emit_unary_intrinsic(UnaryIntrinsic::TYPEVAR)?;
                    }
                }
                TypeParam::ParamSpec(_) => self.emit_unary_intrinsic(UnaryIntrinsic::PARAMSPEC)?,
                TypeParam::TypeVarTuple(_) => {
                    self.emit_unary_intrinsic(UnaryIntrinsic::TYPEVARTUPLE)?;
                }
            }
            if let Some(default) = parameter.default() {
                self.compile_type_parameter_thunk(name, default, default.range(), true)?;
                self.assembler
                    .set_location(self.source_location(parameter.range()));
                self.emit_binary_intrinsic(BinaryIntrinsic::SET_TYPEPARAM_DEFAULT)?;
            }
            self.emit(COPY, 1, 1)?;
            self.store_name(name)?;
        }
        if let Some(first) = type_params.first() {
            self.assembler
                .set_location(self.source_location(first.range()));
        }
        self.emit_build(BUILD_TUPLE, type_params.len())
    }

    fn compile_type_parameter_thunk(
        &mut self,
        name: &str,
        expression: &Expr,
        setup_range: ruff_text_size::TextRange,
        nested: bool,
    ) -> Result<(), CompileError> {
        let mut references = ReferenceCollector::default();
        references.visit_expr(expression);
        let required_names = expression_required_names(expression);
        let mut freevars: BTreeSet<_> = required_names
            .iter()
            .filter(|name| self.is_type_parameter(name) || self.can_provide_closure(name))
            .cloned()
            .collect();
        let can_see_class_scope = matches!(self.unit.scope, Scope::Class { .. })
            || self
                .symbols
                .free_names
                .iter()
                .any(|name| name == "__classdict__");
        if can_see_class_scope {
            freevars.insert("__classdict__".to_string());
        }
        let globals = references
            .references
            .difference(&freevars.iter().cloned().collect())
            .cloned()
            .collect();
        let closure_names: Vec<_> = freevars.iter().cloned().collect();
        let plan = LoweringFunctionPlan {
            locals: vec![".format".to_string()],
            globals,
            freevars,
            ..LoweringFunctionPlan::default()
        };
        // Definitions inside an annotation scope are qualified against that
        // scope's parent. Generic-parameter wrappers are themselves annotation
        // scopes even though this compiler represents them as functions.
        let child_qualified_name_parent = if self.unit.generic_target_qualified_name.is_some() {
            self.unit.qualified_name.clone()
        } else {
            match self.unit.scope {
                Scope::Module => String::new(),
                Scope::Class { .. } => self.unit.qualified_name.clone(),
                Scope::Function { .. } => format!("{}.<locals>", self.unit.qualified_name),
            }
        };
        let mut child = Self::function(
            Arc::clone(&self.context),
            name,
            self.unit
                .generic_target_qualified_name
                .as_ref()
                .map_or_else(
                    || self.child_qualified_name(name),
                    |qualified_name| {
                        qualified_name.rsplit_once('.').map_or_else(
                            || name.to_string(),
                            |(parent, _)| format!("{parent}.{name}"),
                        )
                    },
                ),
            self.line_number(u32::from(setup_range.start())),
            &plan,
            self.unit.scope_id,
            1,
            1,
            0,
            (if nested { CO_NESTED } else { 0 }) | (self.unit.flags & SUPPORTED_FUTURE_FLAGS),
        )?;
        child.unit.private_name.clone_from(&self.unit.private_name);
        child
            .symbols
            .imported_scope_names
            .clone_from(&self.symbols.imported_scope_names);
        child.unit.child_qualified_name_parent = Some(child_qualified_name_parent);
        if can_see_class_scope {
            child.unit.annotation_classdict_index = Some(child.closure_index("__classdict__")?);
        }
        child.emit_function_prologue()?;
        child
            .assembler
            .set_location(child.source_location(setup_range));
        child.emit_annotation_format_guard()?;
        child.compile_expression(expression)?;
        child
            .assembler
            .set_location(child.source_location(setup_range));
        child.emit(RETURN_VALUE, 0, -1)?;
        let child = child.finish_inner(false)?;

        self.assembler
            .set_location(self.source_location(setup_range));
        let defaults = self.add_constant(Constant::Tuple(vec![Constant::Int(1)]))?;
        self.emit(LOAD_CONST, defaults, 1)?;
        if !closure_names.is_empty() {
            self.emit_closure_tuple(&closure_names)?;
        }
        let code = self.add_constant(Constant::Code(Box::new(child)))?;
        self.emit(LOAD_CONST, code, 1)?;
        self.emit(MAKE_FUNCTION, 0, 0)?;
        if !closure_names.is_empty() {
            self.emit_function_attribute(FunctionAttribute::CLOSURE)?;
        }
        self.emit_function_attribute(FunctionAttribute::DEFAULTS)
    }

    fn emit_annotation_format_guard(&mut self) -> Result<(), CompileError> {
        let parameter = self
            .symbols
            .locals
            .first()
            .map_or_else(|| "format".to_string(), Clone::clone);
        self.load_name(&parameter)?;
        self.add_constant(Constant::Int(2))?;
        self.emit(LOAD_SMALL_INT, 2, 1)?;
        self.emit_comparison(ComparisonOperation::GT, false)?;
        let supported = self.assembler.label();
        self.emit_jump_forward(POP_JUMP_IF_FALSE, supported, -1)?;
        self.emit(NOT_TAKEN, 0, 0)?;
        self.emit_common_constant(CommonConstant::NOTIMPLEMENTEDERROR)?;
        self.emit(RAISE_VARARGS, 1, -1)?;
        self.assembler.mark(supported);
        self.set_depth(0);
        Ok(())
    }

    fn compile_function_annotations(
        &self,
        definition: &StmtFunctionDef,
        function_plan: &LoweringFunctionPlan,
    ) -> Result<Option<CodeObject>, CompileError> {
        let parameters = &definition.parameters;
        let mut annotations = Vec::<(String, &Expr)>::new();
        for parameter in parameters.args.iter().chain(&parameters.posonlyargs) {
            if let Some(annotation) = parameter.parameter.annotation.as_deref() {
                annotations.push((parameter.name().as_str().to_string(), annotation));
            }
        }
        if let Some(parameter) = &parameters.vararg
            && let Some(annotation) = parameter.annotation.as_deref()
        {
            annotations.push((parameter.name.as_str().to_string(), annotation));
        }
        for parameter in &parameters.kwonlyargs {
            if let Some(annotation) = parameter.parameter.annotation.as_deref() {
                annotations.push((parameter.name().as_str().to_string(), annotation));
            }
        }
        if let Some(parameter) = &parameters.kwarg
            && let Some(annotation) = parameter.annotation.as_deref()
        {
            annotations.push((parameter.name.as_str().to_string(), annotation));
        }
        if let Some(annotation) = definition.returns.as_deref() {
            annotations.push(("return".to_string(), annotation));
        }
        if annotations.is_empty() {
            return Ok(None);
        }

        let mut freevars = function_plan.annotation_freevars.clone();
        if self.unit.flags & CO_FUTURE_ANNOTATIONS == 0 {
            freevars.extend(
                function_plan
                    .annotation_references
                    .iter()
                    .filter(|name| self.is_type_parameter(name))
                    .cloned(),
            );
        }
        let can_see_class_scope = self.unit.flags & CO_FUTURE_ANNOTATIONS == 0
            && (matches!(self.unit.scope, Scope::Class { .. })
                || self
                    .symbols
                    .free_names
                    .iter()
                    .any(|name| name == "__classdict__"));
        if can_see_class_scope {
            freevars.insert("__classdict__".to_string());
        }
        let plan = LoweringFunctionPlan {
            locals: vec!["format".to_string()],
            freevars,
            ..LoweringFunctionPlan::default()
        };
        let parameter_flags = (if self.child_function_is_nested() {
            CO_NESTED
        } else {
            0
        }) | (self.unit.flags & SUPPORTED_FUTURE_FLAGS);
        let mut child = Self::function(
            Arc::clone(&self.context),
            "__annotate__",
            self.unit
                .generic_target_qualified_name
                .as_ref()
                .map_or_else(
                    || self.child_qualified_name("__annotate__"),
                    |qualified_name| {
                        qualified_name.rsplit_once('.').map_or_else(
                            || "__annotate__".to_string(),
                            |(parent, _)| format!("{parent}.__annotate__"),
                        )
                    },
                ),
            self.line_number(u32::from(definition.name.range().start())),
            &plan,
            self.unit.scope_id,
            1,
            1,
            0,
            parameter_flags,
        )?;
        child.unit.private_name.clone_from(&self.unit.private_name);
        child
            .symbols
            .imported_scope_names
            .clone_from(&self.symbols.imported_scope_names);
        child.unit.annotation_thunk = true;
        if can_see_class_scope {
            child.unit.annotation_classdict_index = Some(child.closure_index("__classdict__")?);
        }
        child.emit_function_prologue()?;
        let location = self.definition_location(
            definition.range,
            definition.name.range(),
            b"def",
            definition.is_async,
        );
        child.assembler.set_location(location);

        child.emit_annotation_format_guard()?;

        let future_annotations = self.unit.flags & CO_FUTURE_ANNOTATIONS != 0;
        for (name, annotation) in &annotations {
            child.assembler.set_location(location);
            let name = child.mangled_name(name);
            child.load_string_constant(&name)?;
            if future_annotations {
                child
                    .assembler
                    .set_location(child.source_location(annotation.range()));
                child.load_string_constant(&unparse_annotation(annotation))?;
            } else if let Expr::Starred(starred) = annotation {
                child.compile_expression(&starred.value)?;
                child.assembler.set_location(location);
                child.emit(UNPACK_SEQUENCE, 1, 0)?;
            } else {
                child.compile_expression(annotation)?;
            }
        }
        child.assembler.set_location(location);
        let count = to_u32(annotations.len(), "annotation count")?;
        child.emit(BUILD_MAP, count, 1 - i32::try_from(count).unwrap() * 2)?;
        child.emit(RETURN_VALUE, 0, -1)?;
        child.finish_inner(false).map(Some)
    }

    pub(super) fn compile_class_annotations(
        &self,
        body: &[Stmt],
    ) -> Result<Option<(CodeObject, Vec<String>)>, CompileError> {
        let mut annotations = Vec::new();
        collect_simple_annotations(body, &mut annotations);
        if annotations.is_empty() {
            return Ok(None);
        }

        let mut references = ReferenceCollector::default();
        for annotation in &annotations {
            references.visit_expr(&annotation.annotation);
        }
        let available_freevars: HashSet<_> = self.symbols.free_names.iter().cloned().collect();
        let mut freevars: BTreeSet<_> = references
            .references
            .iter()
            .filter(|name| available_freevars.contains(*name))
            .cloned()
            .collect();
        freevars.insert("__classdict__".to_string());
        let globals = references
            .references
            .iter()
            .filter(|name| !freevars.contains(*name))
            .cloned()
            .collect();
        let closure_names: Vec<_> = freevars.iter().cloned().collect();
        let plan = LoweringFunctionPlan {
            locals: vec!["format".to_string()],
            globals,
            freevars,
            ..LoweringFunctionPlan::default()
        };
        let parameter_flags =
            if self.child_function_is_nested() || !self.symbols.free_names.is_empty() {
                CO_NESTED
            } else {
                0
            };
        let mut child = Self::function(
            Arc::clone(&self.context),
            "__annotate__",
            self.child_qualified_name("__annotate__"),
            self.unit.first_line_number,
            &plan,
            self.unit.scope_id,
            1,
            1,
            0,
            parameter_flags,
        )?;
        child.unit.private_name.clone_from(&self.unit.private_name);
        child
            .symbols
            .imported_scope_names
            .clone_from(&self.symbols.imported_scope_names);
        child.unit.annotation_thunk = true;
        child.unit.annotation_classdict_index = Some(child.closure_index("__classdict__")?);
        child.emit_function_prologue()?;
        let line = i32::try_from(self.unit.first_line_number).unwrap_or(i32::MAX);
        let setup_location = SourceLocation::new(line, line, 0, 0);
        child.assembler.set_location(setup_location);
        child.emit_annotation_format_guard()?;
        child.emit(BUILD_MAP, 0, 1)?;

        for annotation in annotations {
            let Expr::Name(target) = annotation.target.as_ref() else {
                return Err(unsupported("simple annotation target"));
            };
            child
                .assembler
                .set_location(child.source_location(annotation.annotation.range()));
            child.compile_expression(&annotation.annotation)?;
            child
                .assembler
                .set_location(child.source_location(annotation.range));
            child.emit(COPY, 2, 1)?;
            let target = child.mangled_name(target.id.as_str());
            child.load_string_constant(&target)?;
            child.assembler.set_location(setup_location);
            child.emit(STORE_SUBSCR, 0, -3)?;
        }
        child.assembler.set_location(setup_location);
        child.emit(RETURN_VALUE, 0, -1)?;
        let child = child.finish_inner(false)?;
        Ok(Some((child, closure_names)))
    }

    pub(super) fn compile_module_annotations(
        &self,
        body: &[Stmt],
    ) -> Result<CodeObject, CompileError> {
        let mut annotations = Vec::new();
        collect_simple_annotations(body, &mut annotations);
        let first = annotations.first().ok_or_else(|| {
            CompileError::Internal("module annotation thunk has no annotations".to_string())
        })?;
        let plan = LoweringFunctionPlan {
            locals: vec!["format".to_string()],
            ..LoweringFunctionPlan::default()
        };
        let mut child = Self::function(
            Arc::clone(&self.context),
            "__annotate__",
            "__annotate__".to_string(),
            self.line_number(u32::from(
                body.first().map_or(first.range, Ranged::range).start(),
            )),
            &plan,
            self.unit.scope_id,
            1,
            1,
            0,
            0,
        )?;
        child.unit.private_name.clone_from(&self.unit.private_name);
        child
            .symbols
            .imported_scope_names
            .clone_from(&self.symbols.imported_scope_names);
        child.unit.annotation_thunk = true;
        child.emit_function_prologue()?;
        let setup_location = self.source_location(body.first().map_or(first.range, Ranged::range));
        child.assembler.set_location(setup_location);
        child.load_name("format")?;
        child.add_constant(Constant::Int(2))?;
        child.emit(LOAD_SMALL_INT, 2, 1)?;
        child.emit_comparison(ComparisonOperation::GT, false)?;
        let supported = child.assembler.label();
        child.emit_jump_forward(POP_JUMP_IF_FALSE, supported, -1)?;
        child.emit(NOT_TAKEN, 0, 0)?;
        child.emit_common_constant(CommonConstant::NOTIMPLEMENTEDERROR)?;
        child.emit(RAISE_VARARGS, 1, -1)?;
        child.assembler.mark(supported);
        child.set_depth(0);
        child.emit(BUILD_MAP, 0, 1)?;

        for (index, annotation) in annotations.iter().enumerate() {
            let Expr::Name(target) = annotation.target.as_ref() else {
                return Err(unsupported("simple annotation target"));
            };
            let location = self.source_location(annotation.range);
            child.assembler.set_location(location);
            child.emit(LOAD_SMALL_INT, to_u32(index, "module annotation index")?, 1)?;
            child.load_name("__conditional_annotations__")?;
            child.emit(CONTAINS_OP, 0, -1)?;
            let skip = child.assembler.label();
            child.emit_jump_forward(POP_JUMP_IF_FALSE, skip, -1)?;
            child.emit(NOT_TAKEN, 0, 0)?;
            child.compile_expression(&annotation.annotation)?;
            child.assembler.set_location(location);
            child.emit(COPY, 2, 1)?;
            child.load_string_constant(target.id.as_str())?;
            child.assembler.set_location(setup_location);
            child.emit(STORE_SUBSCR, 0, -3)?;
            child.assembler.mark(skip);
            child.set_depth(1);
        }
        child.assembler.set_location(setup_location);
        child.emit(RETURN_VALUE, 0, -1)?;
        child.finish_inner(false)
    }
}

fn function_has_annotations(definition: &StmtFunctionDef) -> bool {
    let parameters = &definition.parameters;
    definition.returns.is_some()
        || parameters
            .posonlyargs
            .iter()
            .chain(&parameters.args)
            .chain(&parameters.kwonlyargs)
            .any(|parameter| parameter.parameter.annotation.is_some())
        || parameters
            .vararg
            .as_ref()
            .is_some_and(|parameter| parameter.annotation.is_some())
        || parameters
            .kwarg
            .as_ref()
            .is_some_and(|parameter| parameter.annotation.is_some())
}
pub(super) fn clean_doc(docstring: &str) -> String {
    let mut expanded = String::with_capacity(docstring.len());
    let mut column = 0_usize;
    for character in docstring.chars() {
        match character {
            '\t' => {
                let spaces = 8 - column % 8;
                expanded.extend(std::iter::repeat_n(' ', spaces));
                column += spaces;
            }
            '\n' | '\r' => {
                expanded.push(character);
                column = 0;
            }
            _ => {
                expanded.push(character);
                column += 1;
            }
        }
    }

    let lines: Vec<_> = expanded.split_inclusive('\n').collect();
    let margin = lines
        .iter()
        .skip(1)
        .filter_map(|line| {
            let indentation = line.bytes().take_while(|byte| *byte == b' ').count();
            line.as_bytes()
                .get(indentation)
                .is_some_and(|byte| *byte != b'\n')
                .then_some(indentation)
        })
        .min()
        .unwrap_or(0);

    let Some((first, remaining)) = lines.split_first() else {
        return expanded;
    };
    let mut cleaned = String::with_capacity(expanded.len());
    cleaned.push_str(first.trim_start_matches(' '));
    for line in remaining {
        let indentation = line.bytes().take_while(|byte| *byte == b' ').count();
        cleaned.push_str(&line[indentation.min(margin)..]);
    }
    cleaned
}
