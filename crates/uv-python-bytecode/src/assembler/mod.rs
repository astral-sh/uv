mod block_graph;
mod ir;
mod layout;
mod passes;

#[cfg(test)]
mod tests;

use rustc_hash::{FxHashMap, FxHashSet};

use crate::target::opcodes::{
    CHECK_EXC_MATCH, COMPARE_OP, CONTAINS_OP, COPY, DELETE_DEREF, DELETE_FAST, DICT_MERGE,
    DICT_UPDATE, END_SEND, EXTENDED_ARG, FOR_ITER, FORMAT_SIMPLE, GET_ANEXT, GET_LEN,
    GET_YIELD_FROM_ITER, IMPORT_FROM, IS_OP, JUMP_BACKWARD, JUMP_BACKWARD_NO_INTERRUPT,
    JUMP_FORWARD, LIST_APPEND, LIST_EXTEND, LOAD_ATTR, LOAD_COMMON_CONSTANT, LOAD_CONST, LOAD_FAST,
    LOAD_FAST_AND_CLEAR, LOAD_FAST_BORROW, LOAD_FAST_BORROW_LOAD_FAST_BORROW, LOAD_FAST_CHECK,
    LOAD_FAST_LOAD_FAST, LOAD_GLOBAL, LOAD_SMALL_INT, LOAD_SPECIAL, LOAD_SUPER_ATTR, MAP_ADD,
    MATCH_KEYS, MATCH_MAPPING, MATCH_SEQUENCE, NOP, NOT_TAKEN, POP_EXCEPT, POP_JUMP_IF_FALSE,
    POP_JUMP_IF_TRUE, POP_TOP, PUSH_EXC_INFO, RERAISE, RETURN_VALUE, SEND, SET_ADD,
    SET_FUNCTION_ATTRIBUTE, SET_UPDATE, STORE_DEREF, STORE_FAST, STORE_FAST_LOAD_FAST,
    STORE_FAST_STORE_FAST, STORE_GLOBAL, SWAP, TO_BOOL, UNARY_NOT, WITH_EXCEPT_START,
};
use crate::target::{
    Opcode, is_conditional_jump, is_scope_exit as ends_scope, is_unconditional_jump,
    num_popped as opcode_num_popped, num_pushed as opcode_num_pushed,
};

#[cfg(any(test, debug_assertions))]
use ir::AssemblerStage;
pub(crate) use ir::{AssembledCode, Assembler, InstructionId, Label, SourceLocation};
use ir::{ExceptionRegion, Instruction, InstructionFlags, Item, Operand};

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

    fn last_instruction_mut(&mut self) -> Option<&mut Instruction> {
        self.items.iter_mut().rev().find_map(|item| match item {
            Item::Instruction(instruction) => Some(instruction),
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
            |item| matches!(item, Item::Instruction(instruction) if instruction.opcode == opcode),
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

    pub(crate) fn prevent_next_borrow_reachability(&mut self) {
        self.next_instruction_borrow_unreachable = true;
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

    pub(crate) fn mark_before_trailing_converted_pop_block(&mut self, label: Label) -> bool {
        let mut instructions = self
            .items
            .iter()
            .enumerate()
            .rev()
            .filter_map(|(index, item)| {
                let Item::Instruction(instruction) = item else {
                    return None;
                };
                Some((index, instruction))
            });
        let Some((_, _)) = instructions.next() else {
            return false;
        };
        let Some((mut index, instruction)) = instructions.next() else {
            return false;
        };
        if !instruction.has_flag(InstructionFlags::CONVERTED_POP_BLOCK) {
            return false;
        }
        if let Some((previous_index, previous)) = instructions.next()
            && previous.opcode == NOP
        {
            index = previous_index;
        }
        self.items.insert(index, Item::Label(label));
        true
    }

    pub(crate) fn fusion_barrier(&mut self) {
        let label = self.label();
        self.mark(label);
    }

    /// Prevents fusion with the next instruction without introducing a CFG boundary.
    pub(crate) fn prevent_last_instruction_fusion(&mut self) {
        if let Some(instruction) = self.last_instruction_mut() {
            instruction.insert_flag(InstructionFlags::PREVENT_FUSION_WITH_NEXT);
        }
    }

    /// Prevents fusion with the previous instruction without introducing a CFG boundary.
    pub(crate) fn prevent_last_instruction_fusion_with_previous(&mut self) {
        if let Some(instruction) = self.last_instruction_mut() {
            instruction.insert_flag(InstructionFlags::PREVENT_FUSION_WITH_PREVIOUS);
        }
    }

    /// Defers a redundant jump until the pass after exit-block duplication.
    pub(crate) fn defer_last_jump_removal(&mut self) {
        if let Some(instruction) = self.last_instruction_mut() {
            instruction.insert_flag(InstructionFlags::DEFER_REDUNDANT_JUMP_REMOVAL);
        }
    }

    /// Allows incoming jumps to thread through the last jump before retaining it as a NOP.
    pub(crate) fn preserve_last_redundant_jump_nop_after_threading(&mut self) {
        if let Some(instruction) = self.last_instruction_mut() {
            instruction.insert_flag(InstructionFlags::DEFER_REDUNDANT_JUMP_REMOVAL);
            instruction.insert_flag(InstructionFlags::PRESERVE_NOP_AFTER_JUMP_THREADING);
        }
    }

    pub(crate) fn prevent_last_jump_inlining(&mut self) {
        if let Some(instruction) = self.last_instruction_mut() {
            instruction.remove_flag(InstructionFlags::INLINE_SMALL_EXIT);
        }
    }

    pub(crate) fn preserve_last_inlined_jump_nop(&mut self) {
        if let Some(instruction) = self.last_instruction_mut() {
            instruction.insert_flag(InstructionFlags::PRESERVE_INLINED_JUMP_NOP);
        }
    }

    pub(crate) fn preserve_last_direct_inlined_jump_nop(&mut self) {
        if let Some(instruction) = self.last_instruction_mut()
            && let Operand::Forward(target) = instruction.operand
        {
            instruction.preserve_direct_inlined_jump_nop = Some(target);
        }
    }

    pub(crate) fn prevent_last_jump_threading_target(&mut self) {
        if let Some(instruction) = self.last_instruction_mut() {
            instruction.remove_flag(InstructionFlags::ALLOW_JUMP_THREADING_TARGET);
        }
    }

    pub(crate) fn prepare_last_no_location_block_for_inlining(&mut self) {
        if let Some(instruction) = self.last_instruction_mut() {
            instruction.location = SourceLocation::NONE;
            instruction.remove_flag(InstructionFlags::ALLOW_JUMP_THREADING_TARGET);
            instruction.insert_flag(InstructionFlags::ALLOW_NO_LOCATION_BLOCK_INLINING);
        }
    }

    pub(crate) fn preserve_last_no_location(&mut self) {
        if let Some(instruction) = self.last_instruction_mut() {
            instruction.insert_flag(InstructionFlags::PRESERVE_NO_LOCATION);
        }
    }

    pub(crate) fn mark_last_as_converted_pop_block(&mut self) {
        if let Some(instruction) = self.last_instruction_mut() {
            instruction.insert_flag(InstructionFlags::CONVERTED_POP_BLOCK);
        }
    }

    /// Removes a trailing NOP and returns its location and immediate exclusion labels, if any.
    pub(crate) fn take_trailing_nop_location(
        &mut self,
    ) -> Option<(SourceLocation, Option<(Label, Label)>)> {
        let index = self
            .items
            .iter()
            .rposition(|item| matches!(item, Item::Instruction(_)))?;
        let Item::Instruction(instruction) = self.items[index] else {
            unreachable!();
        };
        if instruction.opcode != NOP {
            return None;
        }
        if instruction.location.line >= 0
            && self.location.line >= 0
            && instruction.location.line != self.location.line
        {
            return None;
        }
        let exclusion = if index > 0 {
            match (self.items[index - 1], self.items.get(index + 1).copied()) {
                (Item::Label(start), Some(Item::Label(end))) => Some((start, end)),
                _ => None,
            }
        } else {
            None
        };
        self.items.remove(index);
        Some((instruction.location, exclusion))
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
        if let Some(instruction) = self.last_instruction_mut() {
            instruction.insert_flag(InstructionFlags::EXCLUDE_EXCEPTION_IF_EXTENDED);
        }
    }

    pub(crate) fn exclude_last_instruction_from_exception(&mut self) {
        if let Some(instruction) = self.last_instruction_mut() {
            instruction.insert_flag(InstructionFlags::EXCLUDE_EXCEPTION);
        }
    }

    pub(crate) fn set_last_normalized_exception_owner(&mut self, has_owner: bool) {
        if let Some(instruction) = self.last_instruction_mut() {
            instruction.normalized_exception_owner = Some(has_owner);
        }
    }

    pub(crate) fn emit(&mut self, opcode: Opcode, argument: u32) {
        self.emit_operand(opcode, Operand::Value(argument));
    }

    pub(crate) fn emit_with_depth(&mut self, opcode: Opcode, argument: u32, depth_after: u32) {
        self.emit_operand_with_depth(opcode, Operand::Value(argument), Some(depth_after));
    }

    pub(crate) fn emit_owned_fast_with_depth(&mut self, argument: u32, depth_after: u32) {
        self.emit_operand_with_depth_and_ownership(
            LOAD_FAST,
            Operand::Value(argument),
            Some(depth_after),
            true,
        );
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

    pub(crate) fn used_constant_indices(&self, load_const: Opcode) -> FxHashSet<u32> {
        let mut used = FxHashSet::default();
        let reachable = self.reachable_items();
        for (index, item) in self.items.iter().enumerate() {
            let Item::Instruction(instruction) = item else {
                continue;
            };
            if reachable[index]
                && instruction.opcode == load_const
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
            if instruction.opcode != load_const {
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
        let is_constant_load =
            |opcode| matches!(opcode, LOAD_CONST | LOAD_COMMON_CONSTANT | LOAD_SMALL_INT);

        for index in 0..self.items.len().saturating_sub(1) {
            let [Item::Instruction(load), Item::Instruction(pop)] =
                &mut self.items[index..index + 2]
            else {
                continue;
            };
            if !is_constant_load(load.opcode) || pop.opcode != POP_TOP {
                continue;
            }
            load.opcode = NOP;
            load.operand = Operand::Value(0);
            pop.opcode = NOP;
            pop.operand = Operand::Value(0);
        }

        let mut label_positions = FxHashMap::default();
        for (index, item) in self.items.iter().enumerate() {
            if let Item::Label(label) = item {
                label_positions.insert(*label, index);
            }
        }
        let mut replacements = Vec::new();
        for index in 0..self.items.len().saturating_sub(1) {
            let [Item::Instruction(load), Item::Instruction(jump)] = self.items[index..index + 2]
            else {
                continue;
            };
            let Operand::Forward(target) = jump.operand else {
                continue;
            };
            if !is_constant_load(load.opcode) || jump.opcode != JUMP_FORWARD {
                continue;
            }
            let Some(target_position) = label_positions.get(&target).copied() else {
                continue;
            };
            let target = self.items[target_position + 1..]
                .iter()
                .filter_map(|item| {
                    if let Item::Instruction(instruction) = item {
                        Some(*instruction)
                    } else {
                        None
                    }
                })
                .take(3)
                .collect::<Vec<_>>();
            let [pop, return_load, return_value] = target.as_slice() else {
                continue;
            };
            if pop.opcode == POP_TOP
                && is_constant_load(return_load.opcode)
                && return_value.opcode == RETURN_VALUE
            {
                replacements.push((index, *return_load, *return_value));
            }
        }
        for (index, return_load, return_value) in replacements {
            self.items[index] = Item::Instruction(return_load);
            self.items[index + 1] = Item::Instruction(return_value);
        }
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
        opcode: Opcode,
        operand: Operand,
        depth_after: Option<u32>,
    ) {
        self.emit_operand_with_depth_and_ownership(opcode, operand, depth_after, false);
    }

    fn emit_operand_with_depth_and_ownership(
        &mut self,
        opcode: Opcode,
        operand: Operand,
        depth_after: Option<u32>,
        explicitly_owned: bool,
    ) {
        let strict_owned_load = self.strict_owned_loads && opcode == LOAD_FAST;
        let force_owned_load = explicitly_owned
            || strict_owned_load
            || (opcode == LOAD_FAST && !self.load_fast_borrowing_enabled);
        let borrow_unreachable_entry =
            std::mem::take(&mut self.next_instruction_borrow_unreachable);
        let mut instruction = Instruction::new(opcode, operand, self.location, depth_after);
        instruction.set_flag(InstructionFlags::FORCE_OWNED_LOAD, force_owned_load);
        instruction.set_flag(InstructionFlags::STRICT_OWNED_LOAD, strict_owned_load);
        instruction.set_flag(
            InstructionFlags::BORROW_UNREACHABLE_ENTRY,
            borrow_unreachable_entry,
        );
        self.items.push(Item::Instruction(instruction));
    }
}

fn opcode_stack_effect(opcode: Opcode, argument: u32) -> i64 {
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
    !ends_scope(instruction.opcode) && !is_unconditional_jump(instruction.opcode)
}

fn cleanup_block_creation_order(block: &[Item]) -> u32 {
    block
        .iter()
        .filter_map(|item| {
            let Item::Label(label) = item else {
                return None;
            };
            Some(label.0)
        })
        .min()
        .expect("cleanup block has a label")
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

fn extended_arg_count(argument: u32) -> u8 {
    match argument {
        0..=0xff => 0,
        0x100..=0xffff => 1,
        0x1_0000..=0xff_ffff => 2,
        _ => 3,
    }
}
