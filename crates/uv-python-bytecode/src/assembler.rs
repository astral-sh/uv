use std::collections::{HashMap, HashSet};

use crate::CompileError;

const EXTENDED_ARG: u8 = 69;

type EncodedCode = (Vec<u8>, Vec<u8>, Vec<u8>, u32, Option<u32>);

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(crate) struct Label(u32);

#[derive(Clone, Copy, Debug)]
pub(crate) struct InstructionId(usize);

#[derive(Clone, Copy, Debug)]
pub(crate) struct Opcode {
    code: u8,
    caches: u8,
}

impl Opcode {
    pub(crate) const fn new(code: u8, caches: u8) -> Self {
        Self { code, caches }
    }

    pub(crate) const fn code(self) -> u8 {
        self.code
    }
}

#[derive(Clone, Copy, Debug)]
pub(crate) enum Operand {
    Value(u32),
    Forward(Label),
    Backward(Label),
}

#[derive(Clone, Copy, Debug)]
struct Instruction {
    opcode: Opcode,
    operand: Operand,
    location: SourceLocation,
    depth_after: Option<u32>,
    force_owned_load: bool,
    strict_owned_load: bool,
    inline_small_exit: bool,
    preserve_inlined_jump_nop: bool,
    preserve_no_location: bool,
    prevent_fusion_with_next: bool,
    prevent_fusion_with_previous: bool,
    defer_redundant_jump_removal: bool,
    // `NOT_TAKEN` is added after CPython labels exception handlers. The new instruction keeps
    // whatever exception target remains in its reused CFG slot, if there is one.
    normalized_exception_owner: Option<bool>,
    // CPython's cold-block optimizer retains stale exception ownership for the short form of
    // certain synthetic handler-exit jumps, but not when the jump needs an `EXTENDED_ARG`.
    exclude_exception_if_extended: bool,
}

#[derive(Clone, Copy, Debug)]
enum Item {
    Instruction(Instruction),
    Label(Label),
}

#[derive(Debug)]
pub(crate) struct Assembler {
    items: Vec<Item>,
    next_label: u32,
    location: SourceLocation,
    exception_regions: Vec<ExceptionRegion>,
    preserved_block_boundaries: HashSet<Label>,
    borrow_unreachable_blocks: HashSet<Label>,
    load_fast_borrowing_enabled: bool,
    strict_owned_loads: bool,
}

#[derive(Clone, Copy, Debug)]
struct ExceptionRegion {
    start: Label,
    end: Label,
    target: Label,
    depth: u32,
    preserve_lasti: bool,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct SourceLocation {
    pub(crate) line: i32,
    pub(crate) end_line: i32,
    pub(crate) column: i32,
    pub(crate) end_column: i32,
}

impl SourceLocation {
    pub(crate) const NONE: Self = Self {
        line: -1,
        end_line: -1,
        column: -1,
        end_column: -1,
    };

    pub(crate) const fn new(line: i32, end_line: i32, column: i32, end_column: i32) -> Self {
        Self {
            line,
            end_line,
            column,
            end_column,
        }
    }
}

impl Default for Assembler {
    fn default() -> Self {
        Self {
            items: Vec::new(),
            next_label: 0,
            location: SourceLocation::NONE,
            exception_regions: Vec::new(),
            preserved_block_boundaries: HashSet::new(),
            borrow_unreachable_blocks: HashSet::new(),
            load_fast_borrowing_enabled: true,
            strict_owned_loads: false,
        }
    }
}

impl Assembler {
    pub(crate) fn disable_load_fast_borrowing(&mut self) {
        self.load_fast_borrowing_enabled = false;
    }

    /// Prevents local loads emitted while enabled from becoming borrowed loads.
    pub(crate) fn set_strict_owned_loads(&mut self, enabled: bool) -> bool {
        std::mem::replace(&mut self.strict_owned_loads, enabled)
    }

    pub(crate) fn location(&self) -> SourceLocation {
        self.location
    }

    pub(crate) fn set_location(&mut self, location: SourceLocation) {
        self.location = location;
    }

    pub(crate) fn last_instruction_location(&self) -> Option<SourceLocation> {
        self.items.iter().rev().find_map(|item| match item {
            Item::Instruction(instruction) => Some(instruction.location),
            Item::Label(_) => None,
        })
    }

    pub(crate) fn instruction_count(&self) -> usize {
        self.items
            .iter()
            .filter(|item| matches!(item, Item::Instruction(_)))
            .count()
    }

    pub(crate) fn contains_opcode(&self, opcode: Opcode) -> bool {
        self.items.iter().any(
            |item| matches!(item, Item::Instruction(instruction) if instruction.opcode.code == opcode.code),
        )
    }

    pub(crate) fn label(&mut self) -> Label {
        let label = Label(self.next_label);
        self.next_label += 1;
        label
    }

    pub(crate) fn mark(&mut self, label: Label) {
        self.items.push(Item::Label(label));
    }

    /// Keep a source CFG boundary after jump threading removes its last incoming edge.
    pub(crate) fn preserve_block_boundary(&mut self, label: Label) {
        self.preserved_block_boundaries.insert(label);
    }

    /// Keeps CPython's block-local borrowed-load optimizer from visiting this block.
    pub(crate) fn prevent_borrow_reachability(&mut self, label: Label) {
        self.borrow_unreachable_blocks.insert(label);
    }

    pub(crate) fn mark_before_trailing_instructions(&mut self, label: Label, count: usize) {
        let index = self
            .items
            .iter()
            .enumerate()
            .rev()
            .filter(|(_, item)| matches!(item, Item::Instruction(_)))
            .nth(count.saturating_sub(1))
            .map_or(self.items.len(), |(index, _)| index);
        self.items.insert(index, Item::Label(label));
    }

    pub(crate) fn fusion_barrier(&mut self) {
        let label = self.label();
        self.mark(label);
    }

    /// Prevents fusion with the next instruction without introducing a CFG boundary.
    pub(crate) fn prevent_last_instruction_fusion(&mut self) {
        if let Some(Item::Instruction(instruction)) = self
            .items
            .iter_mut()
            .rev()
            .find(|item| matches!(item, Item::Instruction(_)))
        {
            instruction.prevent_fusion_with_next = true;
        }
    }

    /// Prevents fusion with the previous instruction without introducing a CFG boundary.
    pub(crate) fn prevent_last_instruction_fusion_with_previous(&mut self) {
        if let Some(Item::Instruction(instruction)) = self
            .items
            .iter_mut()
            .rev()
            .find(|item| matches!(item, Item::Instruction(_)))
        {
            instruction.prevent_fusion_with_previous = true;
        }
    }

    /// Defers a redundant jump until the pass after exit-block duplication.
    pub(crate) fn defer_last_jump_removal(&mut self) {
        if let Some(Item::Instruction(instruction)) = self
            .items
            .iter_mut()
            .rev()
            .find(|item| matches!(item, Item::Instruction(_)))
        {
            instruction.defer_redundant_jump_removal = true;
        }
    }

    pub(crate) fn prevent_last_jump_inlining(&mut self) {
        if let Some(Item::Instruction(instruction)) = self
            .items
            .iter_mut()
            .rev()
            .find(|item| matches!(item, Item::Instruction(_)))
        {
            instruction.inline_small_exit = false;
        }
    }

    pub(crate) fn preserve_last_inlined_jump_nop(&mut self) {
        if let Some(Item::Instruction(instruction)) = self
            .items
            .iter_mut()
            .rev()
            .find(|item| matches!(item, Item::Instruction(_)))
        {
            instruction.preserve_inlined_jump_nop = true;
        }
    }

    pub(crate) fn preserve_last_no_location(&mut self) {
        if let Some(Item::Instruction(instruction)) = self
            .items
            .iter_mut()
            .rev()
            .find(|item| matches!(item, Item::Instruction(_)))
        {
            instruction.preserve_no_location = true;
        }
    }

    pub(crate) fn take_trailing_nop_location(&mut self) -> Option<SourceLocation> {
        let index = self
            .items
            .iter()
            .rposition(|item| matches!(item, Item::Instruction(_)))?;
        let Item::Instruction(instruction) = self.items[index] else {
            unreachable!();
        };
        if instruction.opcode.code != 27 {
            return None;
        }
        if instruction.location.line >= 0
            && self.location.line >= 0
            && instruction.location.line != self.location.line
        {
            return None;
        }
        self.items.remove(index);
        Some(instruction.location)
    }

    pub(crate) fn add_exception_region(
        &mut self,
        start: Label,
        end: Label,
        target: Label,
        depth: u32,
        preserve_lasti: bool,
    ) {
        self.exception_regions.push(ExceptionRegion {
            start,
            end,
            target,
            depth,
            preserve_lasti,
        });
    }

    pub(crate) fn exclude_last_instruction_from_exception_if_extended(&mut self) {
        if let Some(Item::Instruction(instruction)) = self
            .items
            .iter_mut()
            .rev()
            .find(|item| matches!(item, Item::Instruction(_)))
        {
            instruction.exclude_exception_if_extended = true;
        }
    }

    pub(crate) fn set_last_normalized_exception_owner(&mut self, has_owner: bool) {
        if let Some(Item::Instruction(instruction)) = self
            .items
            .iter_mut()
            .rev()
            .find(|item| matches!(item, Item::Instruction(_)))
        {
            instruction.normalized_exception_owner = Some(has_owner);
        }
    }

    pub(crate) fn emit(&mut self, opcode: Opcode, argument: u32) {
        self.emit_operand(opcode, Operand::Value(argument));
    }

    pub(crate) fn emit_with_depth(&mut self, opcode: Opcode, argument: u32, depth_after: u32) {
        self.emit_operand_with_depth(opcode, Operand::Value(argument), Some(depth_after));
    }

    pub(crate) fn emit_placeholder(&mut self, opcode: Opcode) -> InstructionId {
        let id = InstructionId(self.items.len());
        self.emit_operand(opcode, Operand::Value(0));
        id
    }

    pub(crate) fn emit_placeholder_with_depth(
        &mut self,
        opcode: Opcode,
        depth_after: u32,
    ) -> InstructionId {
        let id = InstructionId(self.items.len());
        self.emit_operand_with_depth(opcode, Operand::Value(0), Some(depth_after));
        id
    }

    pub(crate) fn patch_argument(&mut self, id: InstructionId, argument: u32) {
        let Item::Instruction(instruction) = &mut self.items[id.0] else {
            unreachable!("instruction handle points to a label");
        };
        instruction.operand = Operand::Value(argument);
    }

    pub(crate) fn used_constant_indices(&self, load_const: Opcode) -> HashSet<u32> {
        let mut used = HashSet::new();
        let reachable = self.reachable_items();
        for (index, item) in self.items.iter().enumerate() {
            let Item::Instruction(instruction) = item else {
                continue;
            };
            if reachable[index]
                && instruction.opcode.code == load_const.code
                && let Operand::Value(index) = instruction.operand
            {
                used.insert(index);
            }
        }
        used
    }

    pub(crate) fn remap_constant_indices(&mut self, load_const: Opcode, index_map: &[Option<u32>]) {
        for item in &mut self.items {
            let Item::Instruction(instruction) = item else {
                continue;
            };
            if instruction.opcode.code != load_const.code {
                continue;
            }
            let Operand::Value(index) = &mut instruction.operand else {
                unreachable!("LOAD_CONST has a jump operand");
            };
            if let Some(new_index) = index_map[usize::try_from(*index).unwrap()] {
                *index = new_index;
            }
        }
    }

    /// Removes a side-effect-free constant whose value is immediately discarded.
    pub(crate) fn optimize_constant_pops(&mut self) {
        const NOP: u8 = 27;
        const POP_TOP: u8 = 31;
        const LOAD_CONST: u8 = 82;
        const LOAD_COMMON_CONSTANT: u8 = 81;
        const LOAD_SMALL_INT: u8 = 94;

        for index in 0..self.items.len().saturating_sub(1) {
            let [Item::Instruction(load), Item::Instruction(pop)] =
                &mut self.items[index..index + 2]
            else {
                continue;
            };
            if !matches!(
                load.opcode.code,
                LOAD_CONST | LOAD_COMMON_CONSTANT | LOAD_SMALL_INT
            ) || pop.opcode.code != POP_TOP
            {
                continue;
            }
            load.opcode = Opcode::new(NOP, 0);
            load.operand = Operand::Value(0);
            pop.opcode = Opcode::new(NOP, 0);
            pop.operand = Operand::Value(0);
        }
    }

    pub(crate) fn emit_forward(&mut self, opcode: Opcode, label: Label) {
        self.emit_operand(opcode, Operand::Forward(label));
    }

    pub(crate) fn emit_forward_with_depth(
        &mut self,
        opcode: Opcode,
        label: Label,
        depth_after: u32,
    ) {
        self.emit_operand_with_depth(opcode, Operand::Forward(label), Some(depth_after));
    }

    pub(crate) fn emit_backward(&mut self, opcode: Opcode, label: Label) {
        self.emit_operand(opcode, Operand::Backward(label));
    }

    pub(crate) fn emit_backward_with_depth(
        &mut self,
        opcode: Opcode,
        label: Label,
        depth_after: u32,
    ) {
        self.emit_operand_with_depth(opcode, Operand::Backward(label), Some(depth_after));
    }

    fn emit_operand(&mut self, opcode: Opcode, operand: Operand) {
        self.emit_operand_with_depth(opcode, operand, None);
    }

    fn emit_operand_with_depth(
        &mut self,
        mut opcode: Opcode,
        operand: Operand,
        depth_after: Option<u32>,
    ) {
        let strict_owned_load = self.strict_owned_loads && matches!(opcode.code, 84 | 121);
        let force_owned_load = strict_owned_load
            || opcode.code == 121
            || (opcode.code == 84 && !self.load_fast_borrowing_enabled);
        if force_owned_load {
            opcode = Opcode::new(84, 0);
        }
        self.items.push(Item::Instruction(Instruction {
            opcode,
            operand,
            location: self.location,
            depth_after,
            force_owned_load,
            strict_owned_load,
            inline_small_exit: true,
            preserve_inlined_jump_nop: false,
            preserve_no_location: false,
            prevent_fusion_with_next: false,
            prevent_fusion_with_previous: false,
            defer_redundant_jump_removal: false,
            normalized_exception_owner: None,
            exclude_exception_if_extended: false,
        }));
    }

    #[cfg(test)]
    pub(crate) fn finish(self) -> Result<Vec<u8>, CompileError> {
        self.finish_code(1, 0, 0)
            .map(|(bytecode, _, _, _, _)| bytecode)
    }

    pub(crate) fn finish_code(
        mut self,
        first_line_number: u32,
        local_count: usize,
        parameter_count: usize,
    ) -> Result<EncodedCode, CompileError> {
        let mut removed_max_depth = self.remove_unreachable_instructions();
        self.optimize_boolean_conversions();
        self.thread_forward_jumps();
        if let Some(depth) = self.remove_unreachable_instructions() {
            removed_max_depth = Some(removed_max_depth.map_or(depth, |current| current.max(depth)));
        }
        self.remove_redundant_forward_jumps();
        self.optimize_swap_runs();
        self.apply_static_swaps();
        self.duplicate_exit_blocks();
        self.add_checks_for_uninitialized_loads(local_count, parameter_count);
        self.fuse_superinstructions();
        self.push_cold_blocks_to_end();
        self.remove_redundant_checked_loads();
        self.propagate_locations_within_blocks();
        self.remove_redundant_swaps_before_pops();
        self.remove_redundant_nops();
        self.remove_redundant_forward_jumps();
        self.optimize_load_fast();
        let instruction_count = self
            .items
            .iter()
            .filter(|item| matches!(item, Item::Instruction(_)))
            .count();
        let mut extended_args = vec![0_u8; instruction_count];
        let mut resolved_arguments = vec![0_u32; instruction_count];

        for _ in 0..8 {
            let (positions, labels) = self.positions(&extended_args);
            let mut changed = false;
            let mut instruction_index = 0;

            for item in &self.items {
                let Item::Instruction(instruction) = item else {
                    continue;
                };
                let position = positions[instruction_index];
                let opcode_position = position + u32::from(extended_args[instruction_index]);
                let jump_base = opcode_position + 1 + u32::from(instruction.opcode.caches);
                let argument = match instruction.operand {
                    Operand::Value(argument) => argument,
                    Operand::Forward(label) => {
                        let target = *labels.get(&label).ok_or_else(|| {
                            CompileError::Internal("unbound forward jump label".to_string())
                        })?;
                        target.checked_sub(jump_base).ok_or_else(|| {
                            CompileError::Internal("forward jump points backward".to_string())
                        })?
                    }
                    Operand::Backward(label) => {
                        let target = *labels.get(&label).ok_or_else(|| {
                            CompileError::Internal("unbound backward jump label".to_string())
                        })?;
                        jump_base.checked_sub(target).ok_or_else(|| {
                            CompileError::Internal("backward jump points forward".to_string())
                        })?
                    }
                };
                resolved_arguments[instruction_index] = argument;

                let required = extended_arg_count(argument);
                if required != extended_args[instruction_index] {
                    extended_args[instruction_index] = required;
                    changed = true;
                }
                instruction_index += 1;
            }

            if !changed {
                let line_table = self.line_table(&extended_args, first_line_number);
                let exception_table = self.exception_table(&extended_args)?;
                let max_depth = self
                    .items
                    .iter()
                    .filter_map(|item| match item {
                        Item::Instruction(instruction) => instruction.depth_after,
                        Item::Label(_) => None,
                    })
                    .max()
                    .unwrap_or(0);
                return Ok((
                    self.encode(&extended_args, &resolved_arguments),
                    line_table,
                    exception_table,
                    max_depth,
                    removed_max_depth,
                ));
            }
        }

        Err(CompileError::Internal(
            "jump layout did not converge".to_string(),
        ))
    }

    fn remove_unreachable_instructions(&mut self) -> Option<u32> {
        let reachable = self.reachable_items();
        let mut removed_max_depth = None;
        let mut index = 0_usize;
        self.items.retain(|item| {
            let retain = match item {
                Item::Label(_) => true,
                Item::Instruction(_) if reachable[index] => true,
                Item::Instruction(instruction) => {
                    if let Some(depth) = instruction.depth_after {
                        removed_max_depth =
                            Some(removed_max_depth.map_or(depth, |max: u32| max.max(depth)));
                    }
                    false
                }
            };
            index += 1;
            retain
        });
        removed_max_depth
    }

    fn reachable_items(&self) -> Vec<bool> {
        if self.items.is_empty() {
            return Vec::new();
        }

        let mut block_starts = vec![0_usize];
        let mut block_has_instruction = false;
        for (index, item) in self.items.iter().enumerate() {
            match item {
                Item::Label(_) if block_has_instruction => {
                    if block_starts.last().copied() != Some(index) {
                        block_starts.push(index);
                    }
                    block_has_instruction = false;
                }
                Item::Label(_) => {}
                Item::Instruction(instruction) => {
                    block_has_instruction = true;
                    let ends_block = !matches!(instruction.operand, Operand::Value(_))
                        || matches!(instruction.opcode.code, 35 | 104 | 105);
                    if ends_block && index + 1 < self.items.len() {
                        block_starts.push(index + 1);
                        block_has_instruction = false;
                    }
                }
            }
        }
        block_starts.sort_unstable();
        block_starts.dedup();

        let block_ranges = block_starts
            .iter()
            .enumerate()
            .map(|(index, start)| {
                let end = block_starts
                    .get(index + 1)
                    .copied()
                    .unwrap_or(self.items.len());
                (*start, end)
            })
            .collect::<Vec<_>>();
        let mut item_blocks = vec![0_usize; self.items.len()];
        let mut label_blocks = HashMap::new();
        for (block, (start, end)) in block_ranges.iter().copied().enumerate() {
            for index in start..end {
                item_blocks[index] = block;
                if let Item::Label(label) = self.items[index] {
                    label_blocks.insert(label, block);
                }
            }
        }

        let region_blocks = self
            .exception_regions
            .iter()
            .filter_map(|region| {
                Some((
                    *label_blocks.get(&region.start)?,
                    *label_blocks.get(&region.end)?,
                    *label_blocks.get(&region.target)?,
                ))
            })
            .collect::<Vec<_>>();
        let mut reachable_blocks = vec![false; block_ranges.len()];
        let mut pending = vec![0_usize];
        pending.extend(
            region_blocks
                .iter()
                .filter_map(|(start, end, handler)| (start == end).then_some(*handler)),
        );
        while let Some(block) = pending.pop() {
            if reachable_blocks[block] {
                continue;
            }
            reachable_blocks[block] = true;
            let (start, end) = block_ranges[block];
            let items = &self.items[start..end];
            if block_has_fallthrough(items) && block + 1 < block_ranges.len() {
                pending.push(block + 1);
            }
            if let Some(target) = block_jump_target(items)
                && let Some(target) = label_blocks.get(&target)
            {
                pending.push(*target);
            }
            for (region_start, region_end, handler) in &region_blocks {
                if *region_start <= block && block < *region_end {
                    pending.push(*handler);
                }
            }
        }

        self.items
            .iter()
            .enumerate()
            .map(|(index, item)| {
                matches!(item, Item::Label(_)) || reachable_blocks[item_blocks[index]]
            })
            .collect()
    }

    fn optimize_boolean_conversions(&mut self) {
        const NOP: u8 = 27;
        const TO_BOOL: u8 = 39;
        const UNARY_NOT: u8 = 42;
        const COMPARE_OP: u8 = 56;
        const CONTAINS_OP: u8 = 57;
        const IS_OP: u8 = 74;

        for index in 0..self.items.len().saturating_sub(1) {
            let [Item::Instruction(current), Item::Instruction(next)] =
                &mut self.items[index..index + 2]
            else {
                continue;
            };
            match (current.opcode.code, next.opcode.code) {
                (TO_BOOL, TO_BOOL) => {
                    current.opcode = Opcode::new(NOP, 0);
                    current.operand = Operand::Value(0);
                    continue;
                }
                (UNARY_NOT, TO_BOOL) => {
                    current.opcode = Opcode::new(NOP, 0);
                    current.operand = Operand::Value(0);
                    next.opcode = Opcode::new(UNARY_NOT, 0);
                    next.operand = Operand::Value(0);
                    continue;
                }
                (UNARY_NOT, UNARY_NOT) => {
                    current.opcode = Opcode::new(NOP, 0);
                    current.operand = Operand::Value(0);
                    next.opcode = Opcode::new(NOP, 0);
                    next.operand = Operand::Value(0);
                    continue;
                }
                (CONTAINS_OP | IS_OP, UNARY_NOT) => {
                    let Operand::Value(argument) = current.operand else {
                        continue;
                    };
                    let opcode = current.opcode;
                    current.opcode = Opcode::new(NOP, 0);
                    current.operand = Operand::Value(0);
                    next.opcode = opcode;
                    next.operand = Operand::Value(argument ^ 1);
                    continue;
                }
                _ => {}
            }
            if next.opcode.code != TO_BOOL {
                continue;
            }
            let Operand::Value(argument) = current.operand else {
                continue;
            };
            let replacement = match current.opcode.code {
                COMPARE_OP => Some((Opcode::new(COMPARE_OP, 1), argument | 16)),
                CONTAINS_OP => Some((Opcode::new(CONTAINS_OP, 1), argument)),
                IS_OP => Some((Opcode::new(IS_OP, 0), argument)),
                _ => None,
            };
            let Some((opcode, argument)) = replacement else {
                continue;
            };
            current.opcode = Opcode::new(NOP, 0);
            current.operand = Operand::Value(0);
            next.opcode = opcode;
            next.operand = Operand::Value(argument);
        }
    }

    fn thread_forward_jumps(&mut self) {
        const JUMP_BACKWARD: u8 = 75;
        const JUMP_BACKWARD_NO_INTERRUPT: u8 = 76;
        const JUMP_FORWARD: u8 = 77;
        const COPY: u8 = 59;
        const NOP: u8 = 27;
        const NOT_TAKEN: u8 = 28;
        const POP_JUMP_IF_FALSE: u8 = 100;
        const POP_JUMP_IF_TRUE: u8 = 103;
        const POP_TOP: u8 = 31;
        const TO_BOOL: u8 = 39;

        #[derive(Clone, Copy)]
        struct ConditionalTarget {
            opcode: u8,
            operand: Operand,
            fallthrough_index: usize,
        }

        fn is_value_preserving_jump(items: &[Item], index: usize) -> bool {
            let previous = items[..index]
                .iter()
                .rev()
                .filter_map(|item| {
                    let Item::Instruction(instruction) = item else {
                        return None;
                    };
                    (instruction.opcode.code != NOP).then_some(*instruction)
                })
                .take(2)
                .collect::<Vec<_>>();
            matches!(
                previous.as_slice(),
                [to_bool, copy]
                    if to_bool.opcode.code == TO_BOOL
                        && copy.opcode.code == COPY
                        && matches!(copy.operand, Operand::Value(1))
            )
        }

        // CPython threads the value-preserving jumps used by boolean
        // expressions through a nested boolean test. If both tests jump on
        // the same truth value, they share the final target. Otherwise the
        // inner jump skips the redundant outer test and enters its POP_TOP
        // fallthrough path directly.
        let mut conditional_targets = HashMap::new();
        for (index, item) in self.items.iter().enumerate() {
            let Item::Label(label) = item else {
                continue;
            };
            let following = self.items[index + 1..]
                .iter()
                .enumerate()
                .filter_map(|(offset, item)| {
                    let Item::Instruction(instruction) = item else {
                        return None;
                    };
                    (instruction.opcode.code != NOP).then_some((index + 1 + offset, *instruction))
                })
                .take(5)
                .collect::<Vec<_>>();
            let [
                (_, copy),
                (_, to_bool),
                (_, jump),
                (_, not_taken),
                (fallthrough_index, pop),
            ] = following.as_slice()
            else {
                continue;
            };
            if copy.opcode.code != COPY
                || !matches!(copy.operand, Operand::Value(1))
                || to_bool.opcode.code != TO_BOOL
                || !matches!(jump.opcode.code, POP_JUMP_IF_FALSE | POP_JUMP_IF_TRUE)
                || !matches!(jump.operand, Operand::Forward(_))
                || not_taken.opcode.code != NOT_TAKEN
                || pop.opcode.code != POP_TOP
            {
                continue;
            }
            conditional_targets.insert(
                *label,
                ConditionalTarget {
                    opcode: jump.opcode.code,
                    operand: jump.operand,
                    fallthrough_index: *fallthrough_index,
                },
            );
        }

        let mut fallthrough_indices = self
            .items
            .iter()
            .enumerate()
            .filter_map(|(index, item)| {
                let Item::Instruction(instruction) = item else {
                    return None;
                };
                if !is_value_preserving_jump(&self.items, index) {
                    return None;
                }
                let Operand::Forward(target) = instruction.operand else {
                    return None;
                };
                let target = conditional_targets.get(&target)?;
                (instruction.opcode.code != target.opcode).then_some(target.fallthrough_index)
            })
            .collect::<Vec<_>>();
        fallthrough_indices.sort_unstable();
        fallthrough_indices.dedup();
        let fallthrough_labels = fallthrough_indices
            .iter()
            .map(|index| (*index, self.label()))
            .collect::<HashMap<_, _>>();
        for index in fallthrough_indices.into_iter().rev() {
            self.items
                .insert(index, Item::Label(fallthrough_labels[&index]));
        }

        let value_preserving_jumps = (0..self.items.len())
            .filter(|index| is_value_preserving_jump(&self.items, *index))
            .collect::<HashSet<_>>();
        for (index, item) in self.items.iter_mut().enumerate() {
            let Item::Instruction(instruction) = item else {
                continue;
            };
            if !value_preserving_jumps.contains(&index) {
                continue;
            }
            if !matches!(
                instruction.opcode.code,
                POP_JUMP_IF_FALSE | POP_JUMP_IF_TRUE
            ) {
                continue;
            }
            let Operand::Forward(mut target) = instruction.operand else {
                continue;
            };
            for _ in 0..conditional_targets.len() {
                let Some(next) = conditional_targets.get(&target) else {
                    break;
                };
                if instruction.opcode.code == next.opcode {
                    instruction.operand = next.operand;
                    let Operand::Forward(next_target) = next.operand else {
                        break;
                    };
                    if next_target == target {
                        break;
                    }
                    target = next_target;
                } else {
                    let Some(fallthrough) = fallthrough_labels.get(&next.fallthrough_index) else {
                        break;
                    };
                    instruction.operand = Operand::Forward(*fallthrough);
                    break;
                }
            }
        }

        let mut jump_targets = HashMap::new();
        for (index, item) in self.items.iter().enumerate() {
            let Item::Label(label) = item else {
                continue;
            };
            let Some(Item::Instruction(instruction)) = self.items[index + 1..]
                .iter()
                .find(|item| matches!(item, Item::Instruction(_)))
            else {
                continue;
            };
            // A jump retained as a source-position NOP remains a CFG boundary.
            if !instruction.preserve_inlined_jump_nop
                && matches!(instruction.opcode.code, 75..=77)
                && let operand @ (Operand::Forward(_) | Operand::Backward(_)) = instruction.operand
            {
                jump_targets.insert(*label, (operand, instruction.opcode.code));
            }
        }

        for item in &mut self.items {
            let Item::Instruction(instruction) = item else {
                continue;
            };
            if instruction.opcode.code != JUMP_FORWARD || instruction.preserve_inlined_jump_nop {
                continue;
            }
            let Operand::Forward(mut target) = instruction.operand else {
                continue;
            };
            let mut operand = instruction.operand;
            let mut opcode = instruction.opcode;
            for _ in 0..jump_targets.len() {
                let Some((next_operand, next_opcode)) = jump_targets.get(&target).copied() else {
                    break;
                };
                let next = match next_operand {
                    Operand::Forward(next) | Operand::Backward(next) => next,
                    Operand::Value(_) => unreachable!(),
                };
                if next == target {
                    break;
                }
                operand = next_operand;
                if next_opcode == JUMP_BACKWARD {
                    opcode = Opcode::new(JUMP_BACKWARD, 1);
                } else if next_opcode == JUMP_BACKWARD_NO_INTERRUPT {
                    opcode = Opcode::new(JUMP_BACKWARD_NO_INTERRUPT, 0);
                }
                target = next;
            }
            instruction.operand = operand;
            instruction.opcode = opcode;
        }
    }

    fn remove_redundant_forward_jumps(&mut self) {
        let mut index = 0;
        while index < self.items.len() {
            let Item::Instruction(instruction) = self.items[index] else {
                index += 1;
                continue;
            };
            let Operand::Forward(target) = instruction.operand else {
                index += 1;
                continue;
            };
            if instruction.opcode.code != 77 {
                index += 1;
                continue;
            }
            if instruction.defer_redundant_jump_removal {
                let Item::Instruction(instruction) = &mut self.items[index] else {
                    unreachable!();
                };
                instruction.defer_redundant_jump_removal = false;
                index += 1;
                continue;
            }
            let target_is_next = self.items[index + 1..]
                .iter()
                .take_while(|item| matches!(item, Item::Label(_)))
                .any(|item| matches!(item, Item::Label(label) if *label == target));
            if !target_is_next {
                index += 1;
                continue;
            }
            let previous_line = self.items[..index].iter().rev().find_map(|item| {
                if let Item::Instruction(instruction) = item {
                    Some(instruction.location.line)
                } else {
                    None
                }
            });
            let next_line = self.items[index + 1..].iter().find_map(|item| {
                if let Item::Instruction(instruction) = item {
                    Some(instruction.location.line)
                } else {
                    None
                }
            });
            if !instruction.preserve_inlined_jump_nop
                && (instruction.location.line < 0
                    || previous_line == Some(instruction.location.line)
                    || next_line == Some(instruction.location.line))
            {
                self.items.remove(index);
                if instruction.exclude_exception_if_extended
                    && let Some(Item::Instruction(previous)) = self.items[..index]
                        .iter_mut()
                        .rev()
                        .find(|item| matches!(item, Item::Instruction(_)))
                {
                    previous.exclude_exception_if_extended = true;
                }
                if instruction.location.line >= 0
                    && let Some(Item::Instruction(next)) = self.items[index..]
                        .iter_mut()
                        .find(|item| matches!(item, Item::Instruction(_)))
                    && next.location.line < 0
                    && !next.preserve_no_location
                {
                    next.location = instruction.location;
                }
            } else {
                let Item::Instruction(instruction) = &mut self.items[index] else {
                    unreachable!();
                };
                instruction.opcode = Opcode::new(27, 0);
                instruction.operand = Operand::Value(0);
                index += 1;
            }
        }
    }

    fn push_cold_blocks_to_end(&mut self) {
        let mut blocks = Vec::<Vec<Item>>::new();
        let mut block = Vec::new();
        for item in std::mem::take(&mut self.items) {
            if matches!(item, Item::Label(_))
                && block
                    .iter()
                    .any(|item| matches!(item, Item::Instruction(_)))
            {
                blocks.push(std::mem::take(&mut block));
            }
            let ends_block = matches!(item, Item::Instruction(instruction) if
                !matches!(instruction.operand, Operand::Value(_))
                    || matches!(instruction.opcode.code, 35 | 104 | 105));
            block.push(item);
            if ends_block {
                blocks.push(std::mem::take(&mut block));
            }
        }
        if !block.is_empty() {
            blocks.push(block);
        }
        if blocks.len() < 2 {
            self.items = blocks.into_iter().flatten().collect();
            return;
        }

        let mut block_labels = Vec::with_capacity(blocks.len());
        for block in &mut blocks {
            let label = block.iter().find_map(|item| {
                if let Item::Label(label) = item {
                    Some(*label)
                } else {
                    None
                }
            });
            let label = if let Some(label) = label {
                label
            } else {
                let label = self.label();
                block.insert(0, Item::Label(label));
                label
            };
            block_labels.push(label);
        }

        let mut label_blocks = HashMap::new();
        for (index, block) in blocks.iter().enumerate() {
            for item in block {
                if let Item::Label(label) = item {
                    label_blocks.insert(*label, index);
                }
            }
        }

        let mut region_memberships = self
            .exception_regions
            .iter()
            .map(|region| {
                let start = label_blocks.get(&region.start).copied().unwrap_or(0);
                let end = label_blocks
                    .get(&region.end)
                    .copied()
                    .unwrap_or(blocks.len());
                (start..end).collect::<Vec<_>>()
            })
            .collect::<Vec<_>>();

        let mut warm = vec![false; blocks.len()];
        let mut stack = vec![0_usize];
        while let Some(index) = stack.pop() {
            if warm[index] {
                continue;
            }
            warm[index] = true;
            if block_has_fallthrough(&blocks[index]) && index + 1 < blocks.len() {
                stack.push(index + 1);
            }
            if let Some(target) = block_jump_target(&blocks[index])
                && let Some(target_index) = label_blocks.get(&target).copied()
            {
                stack.push(target_index);
            }
        }

        let mut cold = vec![false; blocks.len()];
        let mut stack = self
            .exception_regions
            .iter()
            .filter_map(|region| label_blocks.get(&region.target).copied())
            .filter(|index| !warm[*index])
            .collect::<Vec<_>>();
        while let Some(index) = stack.pop() {
            if cold[index] || warm[index] {
                continue;
            }
            cold[index] = true;
            if block_has_fallthrough(&blocks[index]) && index + 1 < blocks.len() {
                stack.push(index + 1);
            }
            if let Some(target) = block_jump_target(&blocks[index])
                && let Some(target_index) = label_blocks.get(&target).copied()
            {
                stack.push(target_index);
            }
        }
        if !cold.iter().any(|cold| *cold) {
            self.items = blocks.into_iter().flatten().collect();
            return;
        }

        let explicit_jumps = (0..blocks.len().saturating_sub(1))
            .filter(|index| {
                cold[*index] && block_has_fallthrough(&blocks[*index]) && warm[*index + 1]
            })
            .collect::<Vec<_>>();
        for index in explicit_jumps.into_iter().rev() {
            let target = if let Some(label) = blocks[index + 1].iter().find_map(|item| {
                if let Item::Label(label) = item {
                    Some(*label)
                } else {
                    None
                }
            }) {
                label
            } else {
                let label = self.label();
                blocks[index + 1].insert(0, Item::Label(label));
                label
            };
            let (location, exclude_exception_if_extended) = blocks[index]
                .iter()
                .rev()
                .find_map(|item| {
                    if let Item::Instruction(instruction) = item {
                        Some((
                            instruction.location,
                            instruction.exclude_exception_if_extended,
                        ))
                    } else {
                        None
                    }
                })
                .unwrap_or((SourceLocation::NONE, false));
            let label = self.label();
            blocks.insert(
                index + 1,
                vec![
                    Item::Label(label),
                    Item::Instruction(Instruction {
                        opcode: Opcode::new(76, 0),
                        operand: Operand::Backward(target),
                        location,
                        depth_after: None,
                        force_owned_load: false,
                        strict_owned_load: false,
                        inline_small_exit: false,
                        preserve_inlined_jump_nop: false,
                        preserve_no_location: false,
                        prevent_fusion_with_next: false,
                        prevent_fusion_with_previous: false,
                        defer_redundant_jump_removal: false,
                        normalized_exception_owner: None,
                        exclude_exception_if_extended,
                    }),
                ],
            );
            block_labels.insert(index + 1, label);
            cold.insert(index + 1, true);
            warm.insert(index + 1, false);
            for members in &mut region_memberships {
                for member in members {
                    if *member > index {
                        *member += 1;
                    }
                }
            }
        }

        let mut order = Vec::with_capacity(blocks.len());
        for expected_cold in [false, true] {
            for old_index in 0..blocks.len() {
                if (expected_cold && cold[old_index]) || (!expected_cold && warm[old_index]) {
                    order.push(old_index);
                }
            }
        }
        let mut reordered = order
            .iter()
            .map(|old_index| blocks[*old_index].clone())
            .collect::<Vec<_>>();

        let mut final_label = None;
        let mut rebuilt_regions = Vec::new();
        let original_regions = std::mem::take(&mut self.exception_regions);
        for (region, members) in original_regions.into_iter().zip(&region_memberships) {
            let mut index = 0;
            while index < order.len() {
                while index < order.len() && !members.contains(&order[index]) {
                    index += 1;
                }
                if index == order.len() {
                    break;
                }
                let start = block_labels[order[index]];
                while index < order.len() && members.contains(&order[index]) {
                    index += 1;
                }
                let end = if index < order.len() {
                    block_labels[order[index]]
                } else if let Some(label) = final_label {
                    label
                } else {
                    let label = self.label();
                    final_label = Some(label);
                    label
                };
                rebuilt_regions.push(ExceptionRegion {
                    start,
                    end,
                    target: region.target,
                    depth: region.depth,
                    preserve_lasti: region.preserve_lasti,
                });
            }
        }
        self.exception_regions = rebuilt_regions;

        let mut label_positions = HashMap::new();
        let mut item_position = 0_usize;
        for block in &reordered {
            for item in block {
                if let Item::Label(label) = item {
                    label_positions.insert(*label, item_position);
                }
                item_position += 1;
            }
        }
        let mut item_position = 0_usize;
        for block in &mut reordered {
            for item in block {
                let Item::Instruction(instruction) = item else {
                    item_position += 1;
                    continue;
                };
                let target = match instruction.operand {
                    Operand::Forward(target) | Operand::Backward(target) => target,
                    Operand::Value(_) => {
                        item_position += 1;
                        continue;
                    }
                };
                let Some(target_position) = label_positions.get(&target).copied() else {
                    item_position += 1;
                    continue;
                };
                if target_position < item_position {
                    instruction.operand = Operand::Backward(target);
                    if instruction.opcode.code == 77 {
                        instruction.opcode = Opcode::new(76, 0);
                    }
                } else {
                    instruction.operand = Operand::Forward(target);
                    if matches!(instruction.opcode.code, 75 | 76) {
                        instruction.opcode = Opcode::new(77, 0);
                    }
                }
                item_position += 1;
            }
        }

        self.items = reordered.into_iter().flatten().collect();
        if let Some(final_label) = final_label {
            self.items.push(Item::Label(final_label));
        }
    }

    fn duplicate_exit_blocks(&mut self) {
        let mut blocks = Vec::<Vec<Item>>::new();
        let mut block = Vec::new();
        for item in std::mem::take(&mut self.items) {
            if matches!(item, Item::Label(_))
                && block
                    .iter()
                    .any(|item| matches!(item, Item::Instruction(_)))
            {
                blocks.push(std::mem::take(&mut block));
            }
            let ends_block = matches!(item, Item::Instruction(instruction) if
                !matches!(instruction.operand, Operand::Value(_))
                    || matches!(instruction.opcode.code, 35 | 104 | 105));
            block.push(item);
            if ends_block {
                blocks.push(std::mem::take(&mut block));
            }
        }
        if !block.is_empty() {
            blocks.push(block);
        }
        if blocks.len() < 2 {
            self.items = blocks.into_iter().flatten().collect();
            return;
        }

        for block in &mut blocks {
            let label = block.iter().find_map(|item| {
                if let Item::Label(label) = item {
                    Some(*label)
                } else {
                    None
                }
            });
            let label = if let Some(label) = label {
                label
            } else {
                let label = self.label();
                block.insert(0, Item::Label(label));
                label
            };
            let _ = label;
        }
        let label_blocks = blocks
            .iter()
            .enumerate()
            .flat_map(|(index, block)| {
                block.iter().filter_map(move |item| {
                    if let Item::Label(label) = item {
                        Some((*label, index))
                    } else {
                        None
                    }
                })
            })
            .collect::<HashMap<_, _>>();
        let exception_boundary_blocks = (0..blocks.len())
            .map(|index| {
                self.exception_regions.iter().any(|region| {
                    [region.start, region.end, region.target]
                        .into_iter()
                        .any(|label| label_blocks.get(&label).copied() == Some(index))
                })
            })
            .collect::<Vec<_>>();
        let exception_handler_blocks = (0..blocks.len())
            .map(|index| {
                self.exception_regions
                    .iter()
                    .any(|region| label_blocks.get(&region.target).copied() == Some(index))
            })
            .collect::<Vec<_>>();
        let region_memberships = self
            .exception_regions
            .iter()
            .map(|region| {
                let start = label_blocks.get(&region.start).copied().unwrap_or(0);
                let end = label_blocks
                    .get(&region.end)
                    .copied()
                    .unwrap_or(blocks.len());
                start..end
            })
            .collect::<Vec<_>>();

        let mut predecessors = vec![0_usize; blocks.len()];
        predecessors[0] = 1;
        for (index, block) in blocks.iter().enumerate() {
            if block_has_fallthrough(block) && index + 1 < blocks.len() {
                predecessors[index + 1] += 1;
            }
            if let Some(target) = block_jump_target(block)
                && let Some(target) = label_blocks.get(&target).copied()
            {
                predecessors[target] += 1;
            }
        }

        let exit_without_unique_predecessor = blocks
            .iter()
            .enumerate()
            .map(|(index, block)| {
                predecessors[index] > 1
                    && block.iter().all(|item| {
                        !matches!(item, Item::Instruction(instruction) if instruction.location.line >= 0)
                    })
                    && block.iter().rev().find_map(|item| {
                        if let Item::Instruction(instruction) = item {
                            Some(matches!(instruction.opcode.code, 35 | 104 | 105))
                        } else {
                            None
                        }
                    }) == Some(true)
            })
            .collect::<Vec<_>>();

        let mut inline_copies = vec![Vec::<Vec<Item>>::new(); blocks.len()];
        let mut target_copies = vec![Vec::<Vec<Item>>::new(); blocks.len()];
        let mut copied_region_exclusions =
            vec![Vec::<(Label, Label)>::new(); self.exception_regions.len()];
        let mut drop_block = vec![false; blocks.len()];
        let mut remaining_predecessors = predecessors.clone();
        for source in 0..blocks.len() {
            let Some(target_label) = block_jump_target(&blocks[source]) else {
                continue;
            };
            let Some(target) = label_blocks.get(&target_label).copied() else {
                continue;
            };
            let source_has_fallthrough = block_has_fallthrough(&blocks[source]);
            let target_pre_fusion_size = blocks[target]
                .iter()
                .filter_map(|item| {
                    let Item::Instruction(instruction) = item else {
                        return None;
                    };
                    let fused_push_null = instruction.opcode.code == 92
                        && matches!(instruction.operand, Operand::Value(argument) if argument & 1 != 0);
                    Some(1 + usize::from(fused_push_null))
                })
                .sum::<usize>();
            let target_is_small_exit = target_pre_fusion_size <= 4
                && blocks[target].iter().rev().find_map(|item| {
                    if let Item::Instruction(instruction) = item {
                        Some(matches!(instruction.opcode.code, 35 | 104 | 105))
                    } else {
                        None
                    }
                }) == Some(true);
            let source_allows_small_exit_inlining = blocks[source]
                .iter()
                .rev()
                .find_map(|item| {
                    if let Item::Instruction(instruction) = item {
                        Some(instruction.inline_small_exit)
                    } else {
                        None
                    }
                })
                .unwrap_or(false);
            let target_contains_end_send = blocks[target].iter().any(|item| {
                matches!(item, Item::Instruction(instruction) if instruction.opcode.code == 10)
            });
            let target_has_no_location = blocks[target].iter().all(|item| {
                !matches!(item, Item::Instruction(instruction) if instruction.location.line >= 0)
            });
            let inline_small_exit = !source_has_fallthrough
                && target_is_small_exit
                && source_allows_small_exit_inlining
                && !target_contains_end_send
                && !exception_handler_blocks[target];
            if !inline_small_exit
                && (!exit_without_unique_predecessor[target] || remaining_predecessors[target] <= 1)
            {
                continue;
            }

            let source_location = blocks[source]
                .iter()
                .rev()
                .find_map(|item| {
                    if let Item::Instruction(instruction) = item {
                        Some(instruction.location)
                    } else {
                        None
                    }
                })
                .unwrap_or(SourceLocation::NONE);
            let mut copied = blocks[target].clone();
            let preserve_inlined_jump_nop = if inline_small_exit && source_location.line >= 0 {
                blocks[source]
                    .iter()
                    .rev()
                    .find_map(|item| {
                        if let Item::Instruction(instruction) = item {
                            Some(instruction.preserve_inlined_jump_nop)
                        } else {
                            None
                        }
                    })
                    .unwrap_or(false)
            } else {
                false
            };
            let copied_label = self.label();
            let mut replaced_entry_label = false;
            for item in &mut copied {
                if let Item::Label(label) = item {
                    if !replaced_entry_label {
                        *label = copied_label;
                        replaced_entry_label = true;
                    } else {
                        *label = self.label();
                    }
                }
            }
            if !replaced_entry_label {
                copied.insert(0, Item::Label(copied_label));
            }
            if preserve_inlined_jump_nop {
                let position = copied
                    .iter()
                    .position(|item| matches!(item, Item::Instruction(_)))
                    .unwrap_or(copied.len());
                copied.insert(
                    position,
                    Item::Instruction(Instruction {
                        opcode: Opcode::new(27, 0),
                        operand: Operand::Value(0),
                        location: source_location,
                        depth_after: None,
                        force_owned_load: false,
                        strict_owned_load: false,
                        inline_small_exit: false,
                        preserve_inlined_jump_nop: false,
                        preserve_no_location: false,
                        prevent_fusion_with_next: false,
                        prevent_fusion_with_previous: false,
                        defer_redundant_jump_removal: false,
                        normalized_exception_owner: None,
                        exclude_exception_if_extended: false,
                    }),
                );
            }
            if target_has_no_location {
                if let Some(Item::Instruction(instruction)) = copied
                    .iter_mut()
                    .find(|item| matches!(item, Item::Instruction(_)))
                    && !instruction.preserve_no_location
                {
                    instruction.location = source_location;
                }
            }
            if inline_small_exit {
                let excluded_regions = region_memberships
                    .iter()
                    .enumerate()
                    .filter_map(|(index, membership)| {
                        (membership.contains(&source) && !membership.contains(&target))
                            .then_some(index)
                    })
                    .collect::<Vec<_>>();
                if !excluded_regions.is_empty() {
                    let mut exclusion_start = copied_label;
                    let trailing_nop_position = blocks[source]
                        .iter()
                        .enumerate()
                        .rev()
                        .filter_map(|(position, item)| {
                            if let Item::Instruction(instruction) = item {
                                Some((position, instruction.opcode.code))
                            } else {
                                None
                            }
                        })
                        .nth(1)
                        .filter(|(_, opcode)| *opcode == 27)
                        .map(|(position, _)| position);
                    if let Some(position) = trailing_nop_position {
                        exclusion_start = self.label();
                        blocks[source].insert(position, Item::Label(exclusion_start));
                    }
                    let copied_end = self.label();
                    copied.push(Item::Label(copied_end));
                    for index in excluded_regions {
                        copied_region_exclusions[index].push((exclusion_start, copied_end));
                    }
                }
            }
            if inline_small_exit {
                if let Some(position) = blocks[source]
                    .iter()
                    .rposition(|item| matches!(item, Item::Instruction(_)))
                {
                    blocks[source].remove(position);
                }
            } else if let Some(Item::Instruction(instruction)) = blocks[source]
                .iter_mut()
                .rev()
                .find(|item| matches!(item, Item::Instruction(_)))
            {
                instruction.operand = Operand::Forward(copied_label);
            }
            if source_has_fallthrough {
                target_copies[target].push(copied);
            } else {
                inline_copies[source].push(copied);
            }
            remaining_predecessors[target] -= 1;
            if remaining_predecessors[target] == 0 {
                drop_block[target] = true;
            }
        }

        if copied_region_exclusions
            .iter()
            .any(|exclusions| !exclusions.is_empty())
        {
            let original_regions = std::mem::take(&mut self.exception_regions);
            for (region, exclusions) in original_regions.into_iter().zip(copied_region_exclusions) {
                let mut start = region.start;
                for (exclusion_start, exclusion_end) in exclusions {
                    self.exception_regions.push(ExceptionRegion {
                        start,
                        end: exclusion_start,
                        target: region.target,
                        depth: region.depth,
                        preserve_lasti: region.preserve_lasti,
                    });
                    start = exclusion_end;
                }
                self.exception_regions.push(ExceptionRegion {
                    start,
                    end: region.end,
                    target: region.target,
                    depth: region.depth,
                    preserve_lasti: region.preserve_lasti,
                });
            }
        }

        for target in 0..blocks.len() {
            if exception_handler_blocks[target] {
                continue;
            }
            if !blocks[target].iter().all(|item| {
                !matches!(item, Item::Instruction(instruction) if instruction.location.line >= 0)
            }) {
                continue;
            }
            let predecessor = if target > 0 && block_has_fallthrough(&blocks[target - 1]) {
                Some(target - 1)
            } else {
                blocks.iter().enumerate().find_map(|(source, block)| {
                    block_jump_target(block)
                        .and_then(|label| label_blocks.get(&label).copied())
                        .filter(|index| *index == target)
                        .map(|_| source)
                })
            };
            let Some(location) = predecessor.and_then(|source| {
                blocks[source].iter().rev().find_map(|item| {
                    if let Item::Instruction(instruction) = item {
                        Some(instruction.location)
                    } else {
                        None
                    }
                })
            }) else {
                continue;
            };
            if let Some(Item::Instruction(instruction)) = blocks[target]
                .iter_mut()
                .find(|item| matches!(item, Item::Instruction(_)))
                && !instruction.preserve_no_location
            {
                instruction.location = location;
            }
        }

        let mut reordered = Vec::new();
        for (index, block) in blocks.into_iter().enumerate() {
            if !drop_block[index] {
                reordered.extend(block);
            } else if exception_boundary_blocks[index] {
                reordered.extend(
                    block
                        .into_iter()
                        .filter(|item| matches!(item, Item::Label(_))),
                );
            }
            for copied in inline_copies[index].drain(..) {
                reordered.extend(copied);
            }
            for copied in target_copies[index].drain(..).rev() {
                reordered.extend(copied);
            }
        }
        self.items = reordered;
    }

    fn propagate_locations_within_blocks(&mut self) {
        let mut previous = None;
        let mut block_has_instruction = false;
        for item in &mut self.items {
            match item {
                Item::Label(_) if block_has_instruction => {
                    previous = None;
                    block_has_instruction = false;
                }
                Item::Label(_) => {}
                Item::Instruction(instruction) => {
                    block_has_instruction = true;
                    if instruction.location.line < 0 && !instruction.preserve_no_location {
                        if let Some(location) = previous {
                            instruction.location = location;
                        }
                    } else {
                        previous = Some(instruction.location);
                    }
                    if !matches!(instruction.operand, Operand::Value(_))
                        || matches!(instruction.opcode.code, 35 | 104 | 105)
                    {
                        previous = None;
                        block_has_instruction = false;
                    }
                }
            }
        }
    }

    fn remove_redundant_nops(&mut self) {
        let mut index = 0;
        while index < self.items.len() {
            let Item::Instruction(instruction) = self.items[index] else {
                index += 1;
                continue;
            };
            if instruction.opcode.code != 27 {
                index += 1;
                continue;
            }
            if instruction.location.line < 0 {
                self.items.remove(index);
                continue;
            }
            let next_location = self.items[index + 1..].iter().find_map(|item| {
                if let Item::Instruction(instruction) = item {
                    Some(instruction.location)
                } else {
                    None
                }
            });
            if next_location.is_some_and(|location| location.line == instruction.location.line) {
                self.items.remove(index);
            } else {
                index += 1;
            }
        }
    }

    fn remove_redundant_swaps_before_pops(&mut self) {
        let mut index = 0;
        while index < self.items.len() {
            let Item::Instruction(instruction) = self.items[index] else {
                index += 1;
                continue;
            };
            if instruction.opcode.code != 117 {
                index += 1;
                continue;
            }
            let Operand::Value(depth) = instruction.operand else {
                index += 1;
                continue;
            };
            let pop_count = usize::try_from(depth).unwrap_or(usize::MAX);
            let preceded_by_swap = self.items[..index].iter().rev().find_map(|item| {
                if let Item::Instruction(instruction) = item {
                    Some(instruction.opcode.code == 117)
                } else {
                    None
                }
            }) == Some(true);
            let following = self.items[index + 1..]
                .iter()
                .filter_map(|item| {
                    if let Item::Instruction(instruction) = item {
                        Some(instruction.opcode.code)
                    } else {
                        None
                    }
                })
                .take(pop_count)
                .collect::<Vec<_>>();
            if preceded_by_swap
                && following.len() == pop_count
                && following.iter().all(|opcode| *opcode == 31)
            {
                self.items.remove(index);
            } else {
                index += 1;
            }
        }
    }

    /// Replaces each run of stack swaps with the shortest equivalent sequence.
    ///
    /// This is the first half of CPython's `swaptimize` pass. Reconstructing the
    /// permutation from the end of the run guarantees that the replacement fits
    /// in the original instruction slots.
    fn optimize_swap_runs(&mut self) {
        const NOP: u8 = 27;
        const SWAP: u8 = 117;
        const VISITED: usize = usize::MAX;

        let mut block_labels = HashSet::new();
        for item in &self.items {
            if let Item::Instruction(Instruction {
                operand: Operand::Forward(label) | Operand::Backward(label),
                ..
            }) = item
            {
                block_labels.insert(*label);
            }
        }
        for region in &self.exception_regions {
            block_labels.extend([region.start, region.end, region.target]);
        }
        let mut block_ends = self
            .items
            .iter()
            .enumerate()
            .filter_map(|(index, item)| match item {
                Item::Label(label)
                    if block_labels.contains(label)
                        || self.preserved_block_boundaries.contains(label) =>
                {
                    Some(index)
                }
                Item::Instruction(instruction)
                    if !matches!(instruction.operand, Operand::Value(_))
                        || matches!(instruction.opcode.code, 35 | 104 | 105) =>
                {
                    Some(index + 1)
                }
                _ => None,
            })
            .collect::<Vec<_>>();
        block_ends.push(self.items.len());
        block_ends.sort_unstable();
        block_ends.dedup();

        let mut block_start = 0;
        for block_end in block_ends {
            let instruction_indices = self.items[block_start..block_end]
                .iter()
                .enumerate()
                .filter_map(|(offset, item)| {
                    matches!(item, Item::Instruction(_)).then_some(block_start + offset)
                })
                .collect::<Vec<_>>();
            let mut position = 0;
            while position < instruction_indices.len() {
                let Item::Instruction(first) = self.items[instruction_indices[position]] else {
                    unreachable!();
                };
                if first.opcode.code != SWAP {
                    position += 1;
                    continue;
                }
                let Operand::Value(first_depth) = first.operand else {
                    unreachable!();
                };
                let mut depth = usize::try_from(first_depth).unwrap();
                let mut run_end = position + 1;
                let mut multiple_swaps = false;
                while run_end < instruction_indices.len() {
                    let Item::Instruction(instruction) = self.items[instruction_indices[run_end]]
                    else {
                        unreachable!();
                    };
                    if instruction.opcode.code == SWAP {
                        let Operand::Value(argument) = instruction.operand else {
                            unreachable!();
                        };
                        depth = depth.max(usize::try_from(argument).unwrap());
                        multiple_swaps = true;
                    } else if instruction.opcode.code != NOP {
                        break;
                    }
                    run_end += 1;
                }
                if !multiple_swaps {
                    position += 1;
                    continue;
                }

                let mut stack = (0..depth).collect::<Vec<_>>();
                for instruction_index in &instruction_indices[position..run_end] {
                    let Item::Instruction(instruction) = self.items[*instruction_index] else {
                        unreachable!();
                    };
                    if instruction.opcode.code == SWAP {
                        let Operand::Value(argument) = instruction.operand else {
                            unreachable!();
                        };
                        stack.swap(0, usize::try_from(argument).unwrap() - 1);
                    }
                }

                let mut current = run_end;
                for index in 0..depth {
                    if stack[index] == VISITED || stack[index] == index {
                        continue;
                    }
                    let mut stack_index = index;
                    loop {
                        if stack_index != 0 {
                            current -= 1;
                            let Item::Instruction(instruction) =
                                &mut self.items[instruction_indices[current]]
                            else {
                                unreachable!();
                            };
                            instruction.opcode = Opcode::new(SWAP, 0);
                            instruction.operand =
                                Operand::Value(u32::try_from(stack_index + 1).unwrap_or(u32::MAX));
                        }
                        if stack[stack_index] == VISITED {
                            debug_assert_eq!(stack_index, index);
                            break;
                        }
                        let next = stack[stack_index];
                        stack[stack_index] = VISITED;
                        stack_index = next;
                    }
                }
                while current > position {
                    current -= 1;
                    let Item::Instruction(instruction) =
                        &mut self.items[instruction_indices[current]]
                    else {
                        unreachable!();
                    };
                    instruction.opcode = Opcode::new(NOP, 0);
                    instruction.operand = Operand::Value(0);
                }
                position = run_end;
            }
            block_start = block_end;
        }
    }

    /// Applies stack swaps statically by reordering local stores and pops.
    ///
    /// This is the `apply_static_swaps` half of CPython's `swaptimize` pass and
    /// must run before adjacent local stores are fused. Reverse traversal applies
    /// consecutive swaps from right to left, matching CPython's per-run handoff.
    fn apply_static_swaps(&mut self) {
        const NOP: u8 = 27;
        const POP_TOP: u8 = 31;
        const STORE_FAST: u8 = 112;
        const SWAP: u8 = 117;

        fn swappable(instruction: &Instruction) -> bool {
            matches!(instruction.opcode.code, POP_TOP | STORE_FAST)
        }

        fn stored_local(instruction: &Instruction) -> Option<u32> {
            (instruction.opcode.code == STORE_FAST).then(|| match instruction.operand {
                Operand::Value(argument) => argument,
                Operand::Forward(_) | Operand::Backward(_) => unreachable!(),
            })
        }

        fn next_swappable(
            items: &[Item],
            mut index: usize,
            end: usize,
            line: Option<i32>,
        ) -> Option<usize> {
            while index < end {
                let Item::Instruction(instruction) = items[index] else {
                    index += 1;
                    continue;
                };
                if line.is_some_and(|line| instruction.location.line != line) {
                    return None;
                }
                if instruction.opcode.code == NOP {
                    index += 1;
                    continue;
                }
                return swappable(&instruction).then_some(index);
            }
            None
        }

        let mut block_labels = HashSet::new();
        for item in &self.items {
            if let Item::Instruction(Instruction {
                operand: Operand::Forward(label) | Operand::Backward(label),
                ..
            }) = item
            {
                block_labels.insert(*label);
            }
        }
        for region in &self.exception_regions {
            block_labels.extend([region.start, region.end, region.target]);
        }
        let mut block_ends = self
            .items
            .iter()
            .enumerate()
            .filter_map(|(index, item)| match item {
                Item::Label(label)
                    if block_labels.contains(label)
                        || self.preserved_block_boundaries.contains(label) =>
                {
                    Some(index)
                }
                Item::Instruction(instruction)
                    if !matches!(instruction.operand, Operand::Value(_))
                        || matches!(instruction.opcode.code, 35 | 104 | 105) =>
                {
                    Some(index + 1)
                }
                _ => None,
            })
            .collect::<Vec<_>>();
        block_ends.push(self.items.len());
        block_ends.sort_unstable();
        block_ends.dedup();

        let mut block_start = 0;
        for block_end in block_ends {
            let mut index = block_end;
            while index > block_start {
                index -= 1;
                let Item::Instruction(swap) = self.items[index] else {
                    continue;
                };
                if swap.opcode.code != SWAP {
                    continue;
                }
                let Operand::Value(depth) = swap.operand else {
                    unreachable!();
                };
                let Some(first) = next_swappable(&self.items, index + 1, block_end, None) else {
                    continue;
                };
                let Item::Instruction(first_instruction) = self.items[first] else {
                    unreachable!();
                };
                let line = (first_instruction.location.line >= 0)
                    .then_some(first_instruction.location.line);
                let mut last = first;
                let mut valid = true;
                for _ in 1..depth {
                    let Some(next) = next_swappable(&self.items, last + 1, block_end, line) else {
                        valid = false;
                        break;
                    };
                    last = next;
                }
                if !valid {
                    continue;
                }
                let Item::Instruction(first_instruction) = self.items[first] else {
                    unreachable!();
                };
                let Item::Instruction(last_instruction) = self.items[last] else {
                    unreachable!();
                };
                let first_store = stored_local(&first_instruction);
                let last_store = stored_local(&last_instruction);
                // CPython sees locationless cleanup pops at this stage and
                // therefore does not move a traced pop across them.
                if first_store.is_none() && last_store.is_none() {
                    continue;
                }
                if first_store.is_some() || last_store.is_some() {
                    if first_store == last_store
                        || self.items[first + 1..last].iter().any(|item| {
                            let Item::Instruction(instruction) = item else {
                                return false;
                            };
                            stored_local(instruction).is_some_and(|local| {
                                Some(local) == first_store || Some(local) == last_store
                            })
                        })
                    {
                        continue;
                    }
                }

                let Item::Instruction(swap) = &mut self.items[index] else {
                    unreachable!();
                };
                swap.opcode = Opcode::new(NOP, 0);
                swap.operand = Operand::Value(0);
                self.items.swap(first, last);
            }
            block_start = block_end;
        }
    }

    /// Converts potentially uninitialized local loads to checked loads.
    ///
    /// Mirrors CPython's `add_checks_for_loads_of_uninitialized_variables`
    /// flow-graph pass. A set bit means that a local may be uninitialized on
    /// at least one path into the block.
    fn add_checks_for_uninitialized_loads(&mut self, local_count: usize, parameter_count: usize) {
        const DELETE_FAST: u8 = 63;
        const LOAD_FAST: u8 = 84;
        const LOAD_FAST_AND_CLEAR: u8 = 85;
        const LOAD_FAST_CHECK: u8 = 88;
        const STORE_FAST: u8 = 112;

        if local_count == 0 {
            return;
        }

        let mut block_labels = HashSet::new();
        for item in &self.items {
            if let Item::Instruction(Instruction {
                operand: Operand::Forward(label) | Operand::Backward(label),
                ..
            }) = item
            {
                block_labels.insert(*label);
            }
        }
        for region in &self.exception_regions {
            block_labels.insert(region.target);
        }

        let mut blocks = Vec::<Vec<usize>>::new();
        let mut block = Vec::new();
        for (index, item) in self.items.iter().enumerate() {
            if matches!(item, Item::Label(label) if block_labels.contains(label) || self.preserved_block_boundaries.contains(label))
                && !block.is_empty()
            {
                blocks.push(std::mem::take(&mut block));
            }
            let Item::Instruction(instruction) = item else {
                continue;
            };
            block.push(index);
            if !matches!(instruction.operand, Operand::Value(_))
                || matches!(instruction.opcode.code, 35 | 104 | 105)
            {
                blocks.push(std::mem::take(&mut block));
            }
        }
        if !block.is_empty() {
            blocks.push(block);
        }
        if blocks.is_empty() {
            return;
        }

        let mut item_blocks = vec![None; self.items.len()];
        for (block_index, block) in blocks.iter().enumerate() {
            for index in block {
                item_blocks[*index] = Some(block_index);
            }
        }
        let mut label_blocks = HashMap::new();
        let mut label_positions = HashMap::new();
        let mut next_block = None;
        for (index, item) in self.items.iter().enumerate().rev() {
            match item {
                Item::Instruction(_) => next_block = item_blocks[index],
                Item::Label(label) => {
                    label_positions.insert(*label, index);
                    if block_labels.contains(label)
                        && let Some(block) = next_block
                    {
                        label_blocks.insert(*label, block);
                    }
                }
            }
        }

        let mut exception_successors = HashMap::new();
        for (index, item) in self.items.iter().enumerate() {
            if !matches!(item, Item::Instruction(_)) {
                continue;
            }
            let mut innermost = None::<(usize, usize, usize)>;
            for region in &self.exception_regions {
                let (Some(&start), Some(&end), Some(&target)) = (
                    label_positions.get(&region.start),
                    label_positions.get(&region.end),
                    label_blocks.get(&region.target),
                ) else {
                    continue;
                };
                if start <= index && index < end {
                    let span = end - start;
                    if innermost.is_none_or(|(best_start, best_span, _)| {
                        start > best_start || (start == best_start && span < best_span)
                    }) {
                        innermost = Some((start, span, target));
                    }
                }
            }
            if let Some((_, _, target)) = innermost {
                exception_successors.insert(index, target);
            }
        }

        let mut unsafe_at_entry = vec![vec![false; local_count]; blocks.len()];
        for local in parameter_count.min(local_count)..local_count {
            unsafe_at_entry[0][local] = true;
        }
        let mut needs_check = HashSet::new();
        let mut pending = (0..blocks.len()).collect::<Vec<_>>();
        let mut queued = vec![true; blocks.len()];

        while let Some(block_index) = pending.pop() {
            queued[block_index] = false;
            let mut unsafe_locals = unsafe_at_entry[block_index].clone();
            for local in 64..local_count {
                unsafe_locals[local] = true;
            }

            let mut propagate = |target: usize, state: &[bool]| {
                let mut changed = false;
                for (current, incoming) in unsafe_at_entry[target].iter_mut().zip(state) {
                    if *incoming && !*current {
                        *current = true;
                        changed = true;
                    }
                }
                if changed && !queued[target] {
                    queued[target] = true;
                    pending.push(target);
                }
            };

            for index in &blocks[block_index] {
                if let Some(target) = exception_successors.get(index).copied() {
                    propagate(target, &unsafe_locals);
                }
                let Item::Instruction(instruction) = self.items[*index] else {
                    unreachable!();
                };
                let Operand::Value(argument) = instruction.operand else {
                    continue;
                };
                let Ok(local) = usize::try_from(argument) else {
                    continue;
                };
                if local >= local_count {
                    continue;
                }
                match instruction.opcode.code {
                    DELETE_FAST | LOAD_FAST_AND_CLEAR => unsafe_locals[local] = true,
                    STORE_FAST | LOAD_FAST_CHECK => unsafe_locals[local] = false,
                    LOAD_FAST => {
                        if unsafe_locals[local] {
                            needs_check.insert(*index);
                        }
                        unsafe_locals[local] = false;
                    }
                    _ => {}
                }
            }

            let block_items = blocks[block_index]
                .iter()
                .map(|index| self.items[*index])
                .collect::<Vec<_>>();
            if block_has_fallthrough(&block_items) && block_index + 1 < blocks.len() {
                propagate(block_index + 1, &unsafe_locals);
            }
            if let Some(target) = block_jump_target(&block_items)
                && let Some(target) = label_blocks.get(&target).copied()
            {
                propagate(target, &unsafe_locals);
            }
        }

        for index in needs_check {
            let Item::Instruction(instruction) = &mut self.items[index] else {
                unreachable!();
            };
            if instruction.opcode.code == LOAD_FAST {
                instruction.opcode = Opcode::new(LOAD_FAST_CHECK, 0);
            }
        }
    }

    /// Removes repeated checked loads within a basic block.
    ///
    /// Once a checked load succeeds, CPython's definite-assignment scan treats
    /// that local as initialized for subsequent instructions on the same path.
    fn remove_redundant_checked_loads(&mut self) {
        const DELETE_DEREF: u8 = 62;
        const DELETE_FAST: u8 = 63;
        const LOAD_FAST: u8 = 84;
        const LOAD_FAST_AND_CLEAR: u8 = 85;
        const LOAD_FAST_CHECK: u8 = 88;
        const LOAD_FAST_LOAD_FAST: u8 = 89;
        const STORE_DEREF: u8 = 111;
        const STORE_FAST: u8 = 112;
        const STORE_FAST_LOAD_FAST: u8 = 113;
        const STORE_FAST_STORE_FAST: u8 = 114;

        let mut initialized = HashSet::new();
        let mut block_has_instruction = false;
        for item in &mut self.items {
            let Item::Instruction(instruction) = item else {
                if block_has_instruction {
                    initialized.clear();
                    block_has_instruction = false;
                }
                continue;
            };
            block_has_instruction = true;
            let Operand::Value(argument) = instruction.operand else {
                initialized.clear();
                block_has_instruction = false;
                continue;
            };
            match instruction.opcode.code {
                DELETE_DEREF | DELETE_FAST | LOAD_FAST_AND_CLEAR => {
                    initialized.remove(&argument);
                }
                LOAD_FAST_CHECK => {
                    if initialized.contains(&argument) {
                        instruction.opcode = Opcode::new(LOAD_FAST, 0);
                    }
                    initialized.insert(argument);
                }
                LOAD_FAST | STORE_DEREF | STORE_FAST => {
                    initialized.insert(argument);
                }
                LOAD_FAST_LOAD_FAST | STORE_FAST_LOAD_FAST | STORE_FAST_STORE_FAST => {
                    initialized.extend([argument >> 4, argument & 15]);
                }
                _ => {}
            }
            if matches!(instruction.opcode.code, 35 | 104 | 105) {
                initialized.clear();
                block_has_instruction = false;
            }
        }
    }

    /// Mirrors CPython's final `optimize_load_fast` CFG pass.
    ///
    /// An owned local load can become a borrowed load only when its value is
    /// consumed in the same basic block, is not stored into another local, and
    /// remains supported by the original local until it is consumed.
    fn optimize_load_fast(&mut self) {
        const LOAD_FAST: u8 = 84;
        const LOAD_FAST_AND_CLEAR: u8 = 85;
        const LOAD_FAST_BORROW: u8 = 86;
        const LOAD_FAST_BORROW_LOAD_FAST_BORROW: u8 = 87;
        const LOAD_FAST_LOAD_FAST: u8 = 89;
        const STORE_FAST: u8 = 112;
        const STORE_FAST_LOAD_FAST: u8 = 113;
        const STORE_FAST_STORE_FAST: u8 = 114;

        #[derive(Clone, Copy)]
        struct Reference {
            producer: Option<usize>,
            local: Option<u32>,
        }

        fn pop_reference(stack: &mut Vec<Reference>) -> Reference {
            stack.pop().unwrap_or(Reference {
                producer: None,
                local: None,
            })
        }

        fn kill_local(unsafe_loads: &mut HashSet<usize>, stack: &[Reference], local: u32) {
            for reference in stack {
                if reference.local == Some(local)
                    && let Some(producer) = reference.producer
                {
                    unsafe_loads.insert(producer);
                }
            }
        }

        fn store_local(
            unsafe_loads: &mut HashSet<usize>,
            stack: &[Reference],
            local: u32,
            reference: Reference,
        ) {
            kill_local(unsafe_loads, stack, local);
            if let Some(producer) = reference.producer {
                unsafe_loads.insert(producer);
            }
        }

        let mut block_labels = HashSet::new();
        for item in &self.items {
            if let Item::Instruction(Instruction {
                operand: Operand::Forward(label) | Operand::Backward(label),
                ..
            }) = item
            {
                block_labels.insert(*label);
            }
        }
        // CPython's borrow analysis operates on CFG blocks. Protected-range
        // boundaries do not create blocks, but exception handlers do.
        for region in &self.exception_regions {
            block_labels.insert(region.target);
        }
        block_labels.extend(self.borrow_unreachable_blocks.iter().copied());

        let mut blocks = Vec::<Vec<usize>>::new();
        let mut block = Vec::new();
        for (index, item) in self.items.iter().enumerate() {
            if matches!(item, Item::Label(label) if block_labels.contains(label) || self.preserved_block_boundaries.contains(label))
                && !block.is_empty()
            {
                blocks.push(std::mem::take(&mut block));
            }
            let Item::Instruction(instruction) = item else {
                continue;
            };
            block.push(index);
            if !matches!(instruction.operand, Operand::Value(_))
                || matches!(instruction.opcode.code, 35 | 104 | 105)
                || (instruction.opcode.code == 27 && instruction.preserve_inlined_jump_nop)
            {
                blocks.push(std::mem::take(&mut block));
            }
        }
        if !block.is_empty() {
            blocks.push(block);
        }

        let mut item_blocks = vec![None; self.items.len()];
        for (block_index, block) in blocks.iter().enumerate() {
            for index in block {
                item_blocks[*index] = Some(block_index);
            }
        }
        let mut label_blocks = HashMap::new();
        let mut next_block = None;
        for (index, item) in self.items.iter().enumerate().rev() {
            match item {
                Item::Instruction(_) => next_block = item_blocks[index],
                Item::Label(label) if block_labels.contains(label) => {
                    if let Some(block) = next_block {
                        label_blocks.insert(*label, block);
                    }
                }
                Item::Label(_) => {}
            }
        }
        let borrow_unreachable_blocks = self
            .borrow_unreachable_blocks
            .iter()
            .filter_map(|label| label_blocks.get(label).copied())
            .collect::<HashSet<_>>();
        let mut reachable = vec![false; blocks.len()];
        let mut pending = (!blocks.is_empty())
            .then_some(0)
            .into_iter()
            .collect::<Vec<_>>();
        while let Some(block_index) = pending.pop() {
            if borrow_unreachable_blocks.contains(&block_index) {
                continue;
            }
            if std::mem::replace(&mut reachable[block_index], true) {
                continue;
            }
            let block = &blocks[block_index];
            let Some(last_index) = block.last().copied() else {
                continue;
            };
            let Item::Instruction(last) = self.items[last_index] else {
                unreachable!();
            };
            let folded_jump_has_no_fallthrough =
                last.opcode.code == 27 && last.preserve_inlined_jump_nop;
            if !folded_jump_has_no_fallthrough
                && block_has_fallthrough(&[Item::Instruction(last)])
                && block_index + 1 < blocks.len()
            {
                pending.push(block_index + 1);
            }
            if let Operand::Forward(label) | Operand::Backward(label) = last.operand
                && let Some(target) = label_blocks.get(&label)
            {
                pending.push(*target);
            }
        }

        for (block_index, block) in blocks.into_iter().enumerate() {
            if !reachable[block_index] {
                continue;
            }
            let mut cumulative_effect = 0_i64;
            let mut start_depth = None;
            for index in &block {
                let Item::Instruction(instruction) = self.items[*index] else {
                    unreachable!();
                };
                let Operand::Value(argument) = instruction.operand else {
                    cumulative_effect += opcode_stack_effect(instruction.opcode.code, 0);
                    if let Some(depth_after) = instruction.depth_after {
                        start_depth = Some(i64::from(depth_after) - cumulative_effect);
                        break;
                    }
                    continue;
                };
                cumulative_effect += opcode_stack_effect(instruction.opcode.code, argument);
                if let Some(depth_after) = instruction.depth_after {
                    start_depth = Some(i64::from(depth_after) - cumulative_effect);
                    break;
                }
            }
            let mut stack = vec![
                Reference {
                    producer: None,
                    local: None,
                };
                usize::try_from(start_depth.unwrap_or(0).max(0)).unwrap()
            ];
            let mut unsafe_loads = HashSet::new();

            for index in &block {
                let Item::Instruction(instruction) = self.items[*index] else {
                    unreachable!();
                };
                let argument = match instruction.operand {
                    Operand::Value(argument) => argument,
                    Operand::Forward(_) | Operand::Backward(_) => 0,
                };
                match instruction.opcode.code {
                    63 => kill_local(&mut unsafe_loads, &stack, argument),
                    LOAD_FAST => stack.push(Reference {
                        producer: Some(*index),
                        local: Some(argument),
                    }),
                    LOAD_FAST_AND_CLEAR => {
                        kill_local(&mut unsafe_loads, &stack, argument);
                        stack.push(Reference {
                            producer: Some(*index),
                            local: Some(argument),
                        });
                    }
                    LOAD_FAST_LOAD_FAST => {
                        stack.push(Reference {
                            producer: Some(*index),
                            local: Some(argument >> 4),
                        });
                        stack.push(Reference {
                            producer: Some(*index),
                            local: Some(argument & 15),
                        });
                    }
                    STORE_FAST => {
                        let reference = pop_reference(&mut stack);
                        store_local(&mut unsafe_loads, &stack, argument, reference);
                    }
                    STORE_FAST_LOAD_FAST => {
                        let reference = pop_reference(&mut stack);
                        store_local(&mut unsafe_loads, &stack, argument >> 4, reference);
                        stack.push(Reference {
                            producer: Some(*index),
                            local: Some(argument & 15),
                        });
                    }
                    STORE_FAST_STORE_FAST => {
                        let reference = pop_reference(&mut stack);
                        store_local(&mut unsafe_loads, &stack, argument >> 4, reference);
                        let reference = pop_reference(&mut stack);
                        store_local(&mut unsafe_loads, &stack, argument & 15, reference);
                    }
                    59 => {
                        let reference = stack
                            .len()
                            .checked_sub(usize::try_from(argument).unwrap())
                            .and_then(|index| stack.get(index))
                            .copied()
                            .unwrap_or(Reference {
                                producer: None,
                                local: None,
                            });
                        stack.push(reference);
                    }
                    117 => {
                        if let Some(index) =
                            stack.len().checked_sub(usize::try_from(argument).unwrap())
                        {
                            let top = stack.len().saturating_sub(1);
                            if index < stack.len() {
                                stack.swap(index, top);
                            }
                        }
                    }
                    // These instructions retain all their existing inputs.
                    12 | 15 | 18 | 19 | 24..=26 | 43 | 72 => {
                        let popped = opcode_num_popped(instruction.opcode.code, argument);
                        let pushed = opcode_num_pushed(instruction.opcode.code, argument);
                        // CPython's inner loop shadows the instruction index,
                        // attributing these references to the first
                        // instructions in the block.
                        for producer in block.iter().copied().take(pushed.saturating_sub(popped)) {
                            stack.push(Reference {
                                producer: Some(producer),
                                local: None,
                            });
                        }
                    }
                    // These consume only their top inputs and retain the
                    // container deeper on the stack.
                    66 | 67 | 78 | 79 | 98 | 105 | 107 | 109 => {
                        let popped = opcode_num_popped(instruction.opcode.code, argument);
                        let pushed = opcode_num_pushed(instruction.opcode.code, argument);
                        for _ in 0..popped.saturating_sub(pushed) {
                            pop_reference(&mut stack);
                        }
                    }
                    10 | 108 => {
                        let top = pop_reference(&mut stack);
                        pop_reference(&mut stack);
                        stack.push(top);
                    }
                    6 => {
                        pop_reference(&mut stack);
                        stack.push(Reference {
                            producer: None,
                            local: None,
                        });
                    }
                    70 => stack.push(Reference {
                        producer: None,
                        local: None,
                    }),
                    80 | 96 => {
                        let receiver = pop_reference(&mut stack);
                        if instruction.opcode.code == 96 {
                            pop_reference(&mut stack);
                            pop_reference(&mut stack);
                        }
                        stack.push(Reference {
                            producer: None,
                            local: None,
                        });
                        if argument & 1 != 0 {
                            stack.push(receiver);
                        }
                    }
                    32 | 95 => {
                        let top = pop_reference(&mut stack);
                        stack.push(Reference {
                            producer: None,
                            local: None,
                        });
                        stack.push(top);
                    }
                    106 => {
                        pop_reference(&mut stack);
                        stack.push(Reference {
                            producer: None,
                            local: None,
                        });
                    }
                    _ => {
                        for _ in 0..opcode_num_popped(instruction.opcode.code, argument) {
                            pop_reference(&mut stack);
                        }
                        for _ in 0..opcode_num_pushed(instruction.opcode.code, argument) {
                            stack.push(Reference {
                                producer: None,
                                local: None,
                            });
                        }
                    }
                }
            }

            for reference in &stack {
                if let Some(producer) = reference.producer {
                    unsafe_loads.insert(producer);
                }
            }
            for (position, index) in block.iter().copied().enumerate() {
                if unsafe_loads.contains(&index) {
                    continue;
                }
                let (force_owned_load, strict_owned_load) = match self.items[index] {
                    Item::Instruction(instruction) => {
                        (instruction.force_owned_load, instruction.strict_owned_load)
                    }
                    Item::Label(_) => unreachable!(),
                };
                if strict_owned_load {
                    continue;
                }
                if force_owned_load {
                    let following = block[position + 1..]
                        .iter()
                        .filter_map(|index| match self.items[*index] {
                            Item::Instruction(instruction) if instruction.opcode.code != 27 => {
                                Some(instruction.opcode.code)
                            }
                            _ => None,
                        })
                        .take(2)
                        .collect::<Vec<_>>();
                    let directly_consumed = following
                        .first()
                        .is_some_and(|opcode| matches!(opcode, 35 | 111 | 115))
                        || matches!(following.as_slice(), [31, next] if *next != 29)
                        || matches!(following.as_slice(), [81 | 82 | 94, 56]);
                    if !directly_consumed {
                        continue;
                    }
                }
                let Item::Instruction(instruction) = &mut self.items[index] else {
                    unreachable!();
                };
                match instruction.opcode.code {
                    LOAD_FAST => {
                        instruction.opcode = Opcode::new(LOAD_FAST_BORROW, 0);
                    }
                    LOAD_FAST_LOAD_FAST => {
                        instruction.opcode = Opcode::new(LOAD_FAST_BORROW_LOAD_FAST_BORROW, 0);
                    }
                    _ => {}
                }
            }
        }
    }

    fn fuse_superinstructions(&mut self) {
        const LOAD_FAST: u8 = 84;
        const LOAD_FAST_BORROW: u8 = 86;
        const LOAD_FAST_BORROW_LOAD_FAST_BORROW: u8 = 87;
        const LOAD_FAST_LOAD_FAST: u8 = 89;
        const STORE_FAST: u8 = 112;
        const STORE_FAST_LOAD_FAST: u8 = 113;
        const STORE_FAST_STORE_FAST: u8 = 114;

        let mut fused = Vec::with_capacity(self.items.len());
        let mut index = 0;
        while index < self.items.len() {
            let Some(Item::Instruction(first)) = self.items.get(index).copied() else {
                fused.push(self.items[index]);
                index += 1;
                continue;
            };
            let Some(Item::Instruction(second)) = self.items.get(index + 1).copied() else {
                fused.push(self.items[index]);
                index += 1;
                continue;
            };
            if first.prevent_fusion_with_next || second.prevent_fusion_with_previous {
                fused.push(self.items[index]);
                index += 1;
                continue;
            }
            let (Operand::Value(first_argument), Operand::Value(second_argument)) =
                (first.operand, second.operand)
            else {
                fused.push(self.items[index]);
                index += 1;
                continue;
            };
            if first_argument >= 16 || second_argument >= 16 {
                fused.push(self.items[index]);
                index += 1;
                continue;
            }
            if first.location.line != second.location.line {
                fused.push(self.items[index]);
                index += 1;
                continue;
            }
            let opcode = match (first.opcode.code, second.opcode.code) {
                (LOAD_FAST_BORROW, LOAD_FAST_BORROW) => LOAD_FAST_BORROW_LOAD_FAST_BORROW,
                (LOAD_FAST, LOAD_FAST) => LOAD_FAST_LOAD_FAST,
                (STORE_FAST, LOAD_FAST) | (STORE_FAST, LOAD_FAST_BORROW) => STORE_FAST_LOAD_FAST,
                (STORE_FAST, STORE_FAST) => STORE_FAST_STORE_FAST,
                _ => {
                    fused.push(self.items[index]);
                    index += 1;
                    continue;
                }
            };
            fused.push(Item::Instruction(Instruction {
                opcode: Opcode::new(opcode, 0),
                operand: Operand::Value((first_argument << 4) | second_argument),
                location: first.location,
                depth_after: second.depth_after,
                force_owned_load: first.force_owned_load || second.force_owned_load,
                strict_owned_load: first.strict_owned_load || second.strict_owned_load,
                inline_small_exit: first.inline_small_exit && second.inline_small_exit,
                preserve_inlined_jump_nop: false,
                preserve_no_location: first.preserve_no_location || second.preserve_no_location,
                prevent_fusion_with_next: second.prevent_fusion_with_next,
                prevent_fusion_with_previous: first.prevent_fusion_with_previous,
                defer_redundant_jump_removal: first.defer_redundant_jump_removal
                    || second.defer_redundant_jump_removal,
                normalized_exception_owner: first
                    .normalized_exception_owner
                    .or(second.normalized_exception_owner),
                exclude_exception_if_extended: first.exclude_exception_if_extended
                    || second.exclude_exception_if_extended,
            }));
            index += 2;
        }
        self.items = fused;
    }

    fn exception_table(&self, extended_args: &[u8]) -> Result<Vec<u8>, CompileError> {
        let (positions, labels) = self.positions(extended_args);
        let regions = self
            .exception_regions
            .iter()
            .enumerate()
            .map(|(index, region)| {
                let start = *labels.get(&region.start).ok_or_else(|| {
                    CompileError::Internal("unbound exception region start".to_string())
                })?;
                let end = *labels.get(&region.end).ok_or_else(|| {
                    CompileError::Internal("unbound exception region end".to_string())
                })?;
                let target = *labels.get(&region.target).ok_or_else(|| {
                    CompileError::Internal("unbound exception handler target".to_string())
                })?;
                Ok((
                    start,
                    end,
                    target,
                    region.depth,
                    region.preserve_lasti,
                    index,
                ))
            })
            .collect::<Result<Vec<_>, CompileError>>()?;
        let mut late_exception_exclusions = self
            .items
            .iter()
            .filter_map(|item| {
                let Item::Instruction(instruction) = item else {
                    return None;
                };
                Some(instruction)
            })
            .zip(&positions)
            .zip(extended_args)
            .filter_map(|((instruction, position), extended)| {
                (instruction.exclude_exception_if_extended && *extended > 0).then_some((
                    *position,
                    *position + u32::from(*extended) + 1 + u32::from(instruction.opcode.caches),
                ))
            })
            .collect::<Vec<_>>();
        let mut block_labels = self.preserved_block_boundaries.clone();
        let mut setup_labels = HashSet::new();
        for region in &self.exception_regions {
            block_labels.insert(region.target);
            setup_labels.insert(region.start);
        }
        for item in &self.items {
            if let Item::Instruction(Instruction {
                operand: Operand::Forward(label) | Operand::Backward(label),
                ..
            }) = item
            {
                block_labels.insert(*label);
            }
        }
        let mut instruction_index = 0;
        let mut block_has_instruction = false;
        let mut block_has_stale_exception_owner = false;
        for item in &self.items {
            match item {
                Item::Label(label) => {
                    if block_has_instruction && block_labels.contains(label) {
                        block_has_instruction = false;
                        block_has_stale_exception_owner = false;
                    }
                    if setup_labels.contains(label) {
                        block_has_stale_exception_owner = true;
                    }
                }
                Item::Instruction(instruction) => {
                    if instruction.normalized_exception_owner == Some(false)
                        && !block_has_stale_exception_owner
                    {
                        let position = positions[instruction_index];
                        late_exception_exclusions.push((
                            position,
                            position
                                + u32::from(extended_args[instruction_index])
                                + 1
                                + u32::from(instruction.opcode.caches),
                        ));
                    }
                    if matches!(instruction.opcode.code, 87 | 89 | 113 | 114)
                        || matches!(
                            (instruction.opcode.code, instruction.operand),
                            (92, Operand::Value(argument)) if argument & 1 != 0
                        )
                    {
                        block_has_stale_exception_owner = true;
                    }
                    instruction_index += 1;
                    block_has_instruction = true;
                    if instruction.opcode.code == 28
                        || (!matches!(instruction.opcode.code, 100..=103)
                            && (!matches!(instruction.operand, Operand::Value(_))
                                || matches!(instruction.opcode.code, 35 | 104 | 105)))
                    {
                        block_has_instruction = false;
                        block_has_stale_exception_owner = false;
                    }
                }
            }
        }
        let mut boundaries = regions
            .iter()
            .flat_map(|(start, end, _, _, _, _)| [*start, *end])
            .chain(
                late_exception_exclusions
                    .iter()
                    .flat_map(|(start, end)| [*start, *end]),
            )
            .collect::<Vec<_>>();
        boundaries.sort_unstable();
        boundaries.dedup();

        let mut flattened = Vec::<(u32, u32, u32, u32, bool)>::new();
        for boundary in boundaries.windows(2) {
            let start = boundary[0];
            let end = boundary[1];
            if end <= start {
                continue;
            }
            if late_exception_exclusions
                .iter()
                .any(|(excluded_start, excluded_end)| {
                    *excluded_start <= start && *excluded_end >= end
                })
            {
                continue;
            }
            let Some((_, _, target, depth, preserve_lasti, _)) = regions
                .iter()
                .filter(|(region_start, region_end, _, _, _, _)| {
                    *region_start <= start && *region_end >= end
                })
                .min_by_key(|(region_start, region_end, _, _, _, index)| {
                    (region_end - region_start, *index)
                })
            else {
                continue;
            };
            if let Some((_, previous_end, previous_target, previous_depth, previous_lasti)) =
                flattened.last_mut()
                && *previous_end == start
                && *previous_target == *target
                && *previous_depth == *depth
                && *previous_lasti == *preserve_lasti
            {
                *previous_end = end;
            } else {
                flattened.push((start, end, *target, *depth, *preserve_lasti));
            }
        }

        let mut output = Vec::new();
        for (start, end, target, depth, preserve_lasti) in flattened {
            write_exception_varint(&mut output, start, true);
            write_exception_varint(&mut output, end - start, false);
            write_exception_varint(&mut output, target, false);
            write_exception_varint(&mut output, (depth << 1) | u32::from(preserve_lasti), false);
        }
        Ok(output)
    }

    fn line_table(&self, extended_args: &[u8], first_line_number: u32) -> Vec<u8> {
        let mut locations = Vec::<(SourceLocation, u32)>::new();
        let mut instruction_index = 0_usize;
        for item in &self.items {
            let Item::Instruction(instruction) = item else {
                continue;
            };
            let size = u32::from(extended_args[instruction_index])
                + 1
                + u32::from(instruction.opcode.caches);
            if let Some((location, length)) = locations.last_mut()
                && *location == instruction.location
            {
                *length += size;
            } else {
                locations.push((instruction.location, size));
            }
            instruction_index += 1;
        }

        let mut output = Vec::new();
        let mut current_line = i32::try_from(first_line_number).unwrap_or(i32::MAX);
        for (location, mut length) in locations {
            while length > 8 {
                write_location(&mut output, location, 8, &mut current_line);
                length -= 8;
            }
            if length > 0 {
                write_location(&mut output, location, length, &mut current_line);
            }
        }
        output
    }

    fn positions(&self, extended_args: &[u8]) -> (Vec<u32>, HashMap<Label, u32>) {
        let mut positions = Vec::with_capacity(extended_args.len());
        let mut labels = HashMap::new();
        let mut position = 0_u32;
        let mut instruction_index = 0;

        for item in &self.items {
            match *item {
                Item::Instruction(instruction) => {
                    positions.push(position);
                    position += u32::from(extended_args[instruction_index])
                        + 1
                        + u32::from(instruction.opcode.caches);
                    instruction_index += 1;
                }
                Item::Label(label) => {
                    labels.insert(label, position);
                }
            }
        }

        (positions, labels)
    }

    fn encode(&self, extended_args: &[u8], resolved_arguments: &[u32]) -> Vec<u8> {
        let mut output = Vec::new();
        let mut instruction_index = 0;

        for item in &self.items {
            let Item::Instruction(instruction) = *item else {
                continue;
            };
            let argument = resolved_arguments[instruction_index];
            let extended_arg_count = extended_args[instruction_index];

            for index in (1..=extended_arg_count).rev() {
                output.push(EXTENDED_ARG);
                output.push(((argument >> (u32::from(index) * 8)) & 0xff) as u8);
            }
            output.push(instruction.opcode.code);
            output.push((argument & 0xff) as u8);
            for _ in 0..instruction.opcode.caches {
                output.extend_from_slice(&[0, 0]);
            }

            instruction_index += 1;
        }

        output
    }
}

// Generated from CPython 3.14's `pycore_opcode_metadata.h`. Keeping the pop
// and push counts separate is required by the borrowed-local dataflow pass.
fn opcode_num_popped(opcode: u8, argument: u32) -> usize {
    let argument = usize::try_from(argument).unwrap();
    match opcode {
        0
        | 17
        | 21
        | 22
        | 27
        | 28
        | 33
        | 34
        | 36
        | 60
        | 62..=65
        | 69
        | 75..=77
        | 81..=89
        | 92..=94
        | 97
        | 128 => 0,
        1 | 7 | 37 | 38 | 43 | 96 | 99 => 3,
        2 | 3 | 5 | 6 | 8 | 10 | 44 | 54 | 56 | 57 | 73 | 74 | 106 | 108 | 110 | 114 => 2,
        4 => 4,
        9
        | 11..=16
        | 18..=20
        | 23
        | 25
        | 26
        | 29..=32
        | 35
        | 39..=42
        | 53
        | 58
        | 61
        | 68
        | 70..=72
        | 80
        | 90
        | 91
        | 95
        | 100..=103
        | 111..=113
        | 115
        | 116
        | 118..=120 => 1,
        24 => 2,
        45 => 2 + (argument & 1),
        46 | 48..=51 => argument,
        47 => argument * 2,
        52 => 2 + argument,
        55 => 3 + argument,
        59 | 117 => argument,
        66 => 4 + argument,
        67 | 78 | 79 | 107 | 109 => 1 + argument,
        98 => 2 + argument,
        104 => argument,
        105 => 1 + argument,
        _ => unreachable!("missing stack-pop metadata for opcode {opcode}"),
    }
}

fn opcode_num_pushed(opcode: u8, argument: u32) -> usize {
    let argument = usize::try_from(argument).unwrap();
    match opcode {
        0
        | 3
        | 8
        | 9
        | 11
        | 17
        | 20
        | 27..=31
        | 36..=38
        | 60..=65
        | 68
        | 69
        | 75..=77
        | 97
        | 100..=104
        | 110..=112
        | 114..=116
        | 128 => 0,
        1
        | 2
        | 4
        | 12..=14
        | 16
        | 19
        | 21..=23
        | 35
        | 39..=42
        | 44..=58
        | 71
        | 73
        | 74
        | 81..=86
        | 88
        | 90
        | 91
        | 93
        | 94
        | 99
        | 108
        | 120 => 1,
        5..=7 | 10 | 15 | 18 | 25 | 26 | 32 | 70 | 72 | 87 | 89 | 95 | 106 => 2,
        24 => 3,
        33 | 34 => 1,
        43 => 6,
        59 => argument + 1,
        66 => argument + 3,
        67 | 78 | 79 | 98 | 107 | 109 => argument,
        80 | 92 | 96 => 1 + (argument & 1),
        105 => argument,
        113 => 1,
        117 => argument,
        118 => 1 + (argument & 0xff) + (argument >> 8),
        119 => argument,
        _ => unreachable!("missing stack-push metadata for opcode {opcode}"),
    }
}

fn opcode_stack_effect(opcode: u8, argument: u32) -> i64 {
    i64::try_from(opcode_num_pushed(opcode, argument)).unwrap()
        - i64::try_from(opcode_num_popped(opcode, argument)).unwrap()
}

fn block_has_fallthrough(block: &[Item]) -> bool {
    let Some(instruction) = block.iter().rev().find_map(|item| {
        if let Item::Instruction(instruction) = item {
            Some(instruction)
        } else {
            None
        }
    }) else {
        return true;
    };
    !matches!(instruction.opcode.code, 35 | 75..=77 | 104 | 105)
}

fn block_jump_target(block: &[Item]) -> Option<Label> {
    let instruction = block.iter().rev().find_map(|item| {
        if let Item::Instruction(instruction) = item {
            Some(instruction)
        } else {
            None
        }
    })?;
    match instruction.operand {
        Operand::Forward(target) | Operand::Backward(target) => Some(target),
        Operand::Value(_) => None,
    }
}

fn write_location(
    output: &mut Vec<u8>,
    location: SourceLocation,
    length: u32,
    current_line: &mut i32,
) {
    if location.line < 0 {
        write_location_start(output, 15, length);
        return;
    }
    let line_delta = location.line - *current_line;
    if location.column < 0 || location.end_column < 0 {
        if location.end_line == location.line || location.end_line < 0 {
            write_location_start(output, 13, length);
            write_signed_varint(output, line_delta);
            *current_line = location.line;
            return;
        }
    } else if location.end_line == location.line {
        let column_width = location.end_column - location.column;
        if line_delta == 0 && location.column < 80 && (0..16).contains(&column_width) {
            let column_group = location.column >> 3;
            let column_low_bits = location.column & 7;
            write_location_start(output, column_group, length);
            output.push(
                u8::try_from((column_low_bits << 4) | column_width)
                    .expect("short-form location columns fit in one byte"),
            );
            return;
        }
        if (0..3).contains(&line_delta) && location.column < 128 && location.end_column < 128 {
            write_location_start(output, 10 + line_delta, length);
            output.push(u8::try_from(location.column).expect("one-line column fits in one byte"));
            output
                .push(u8::try_from(location.end_column).expect("one-line column fits in one byte"));
            *current_line = location.line;
            return;
        }
    }

    write_location_start(output, 14, length);
    write_signed_varint(output, line_delta);
    write_varint(output, (location.end_line - location.line).cast_unsigned());
    write_varint(output, (location.column + 1).cast_unsigned());
    write_varint(output, (location.end_column + 1).cast_unsigned());
    *current_line = location.line;
}

fn write_location_start(output: &mut Vec<u8>, code: i32, length: u32) {
    let code = u8::try_from(code).expect("location code is four bits");
    let length = u8::try_from(length).expect("location run is at most eight code units");
    output.push(0x80 | (code << 3) | (length - 1));
}

fn write_signed_varint(output: &mut Vec<u8>, value: i32) {
    let value = if value < 0 {
        ((0_u32.wrapping_sub(value.cast_unsigned())) << 1) | 1
    } else {
        value.cast_unsigned() << 1
    };
    write_varint(output, value);
}

fn write_varint(output: &mut Vec<u8>, mut value: u32) {
    while value >= 64 {
        output.push(64 | (value & 63) as u8);
        value >>= 6;
    }
    output.push(u8::try_from(value).expect("varint tail is six bits"));
}

fn write_exception_varint(output: &mut Vec<u8>, value: u32, mark_entry_start: bool) {
    let mut shift = if value >= 1 << 24 {
        24
    } else if value >= 1 << 18 {
        18
    } else if value >= 1 << 12 {
        12
    } else if value >= 1 << 6 {
        6
    } else {
        0
    };
    let mut first = true;
    loop {
        let mut byte = u8::try_from((value >> shift) & 0x3f).expect("exception varint chunk");
        if shift != 0 {
            byte |= 0x40;
        }
        if first && mark_entry_start {
            byte |= 0x80;
        }
        output.push(byte);
        first = false;
        if shift == 0 {
            break;
        }
        shift -= 6;
    }
}

fn extended_arg_count(argument: u32) -> u8 {
    match argument {
        0..=0xff => 0,
        0x100..=0xffff => 1,
        0x1_0000..=0xff_ffff => 2,
        _ => 3,
    }
}

#[cfg(test)]
mod tests {
    use super::{Assembler, Opcode};

    #[test]
    fn removes_unreachable_jumps() {
        let jump = Opcode::new(77, 0);
        let resume = Opcode::new(128, 0);
        let mut assembler = Assembler::default();
        let start = assembler.label();
        let end = assembler.label();

        assembler.mark(start);
        assembler.emit(resume, 0);
        assembler.emit_forward(jump, end);
        assembler.emit_backward(jump, start);
        assembler.mark(end);

        assert_eq!(assembler.finish().unwrap(), [128, 0]);
    }

    #[test]
    fn emits_extended_arguments() {
        let mut assembler = Assembler::default();
        assembler.emit(Opcode::new(82, 0), 0x1234);
        assert_eq!(assembler.finish().unwrap(), [69, 0x12, 82, 0x34]);
    }
}
