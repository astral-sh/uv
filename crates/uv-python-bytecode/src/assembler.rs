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
    inline_small_exit: bool,
    preserve_inlined_jump_nop: bool,
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
        }
    }
}

impl Assembler {
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

    pub(crate) fn label(&mut self) -> Label {
        let label = Label(self.next_label);
        self.next_label += 1;
        label
    }

    pub(crate) fn mark(&mut self, label: Label) {
        self.items.push(Item::Label(label));
    }

    pub(crate) fn fusion_barrier(&mut self) {
        let label = self.label();
        self.mark(label);
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
        let mut unreachable = false;
        for item in &self.items {
            match item {
                Item::Label(_) => unreachable = false,
                Item::Instruction(_) if unreachable => {}
                Item::Instruction(instruction) => {
                    if instruction.opcode.code == load_const.code
                        && let Operand::Value(index) = instruction.operand
                    {
                        used.insert(index);
                    }
                    unreachable = matches!(instruction.opcode.code, 35 | 75 | 76 | 77 | 104 | 105);
                }
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
        opcode: Opcode,
        operand: Operand,
        depth_after: Option<u32>,
    ) {
        self.items.push(Item::Instruction(Instruction {
            opcode,
            operand,
            location: self.location,
            depth_after,
            inline_small_exit: true,
            preserve_inlined_jump_nop: false,
        }));
    }

    #[cfg(test)]
    pub(crate) fn finish(self) -> Result<Vec<u8>, CompileError> {
        self.finish_code(1).map(|(bytecode, _, _, _, _)| bytecode)
    }

    pub(crate) fn finish_code(
        mut self,
        first_line_number: u32,
    ) -> Result<EncodedCode, CompileError> {
        let removed_max_depth = self.remove_unreachable_instructions();
        self.optimize_boolean_conversions();
        self.thread_forward_jumps();
        self.remove_redundant_forward_jumps();
        self.fuse_superinstructions();
        self.push_cold_blocks_to_end();
        self.duplicate_exit_blocks();
        self.propagate_locations_within_blocks();
        self.remove_redundant_swaps_before_pops();
        self.remove_redundant_nops();
        self.remove_redundant_forward_jumps();
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
        let mut unreachable = false;
        let mut removed_max_depth = None;
        self.items.retain(|item| match item {
            Item::Label(_) => {
                unreachable = false;
                true
            }
            Item::Instruction(instruction) if unreachable => {
                if let Some(depth) = instruction.depth_after {
                    removed_max_depth =
                        Some(removed_max_depth.map_or(depth, |max: u32| max.max(depth)));
                }
                false
            }
            Item::Instruction(instruction) => {
                unreachable = matches!(instruction.opcode.code, 35 | 75 | 76 | 77 | 104 | 105);
                true
            }
        });
        removed_max_depth
    }

    fn optimize_boolean_conversions(&mut self) {
        const NOP: u8 = 27;
        const TO_BOOL: u8 = 39;
        const COMPARE_OP: u8 = 56;
        const CONTAINS_OP: u8 = 57;
        const IS_OP: u8 = 74;

        for index in 0..self.items.len().saturating_sub(1) {
            let [Item::Instruction(current), Item::Instruction(next)] =
                &mut self.items[index..index + 2]
            else {
                continue;
            };
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
            if matches!(instruction.opcode.code, 75..=77)
                && let operand @ (Operand::Forward(_) | Operand::Backward(_)) = instruction.operand
            {
                jump_targets.insert(*label, (operand, instruction.opcode.code));
            }
        }

        for item in &mut self.items {
            let Item::Instruction(instruction) = item else {
                continue;
            };
            if instruction.opcode.code != JUMP_FORWARD {
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
            if instruction.location.line < 0
                || previous_line == Some(instruction.location.line)
                || next_line == Some(instruction.location.line)
            {
                self.items.remove(index);
                if instruction.location.line >= 0
                    && let Some(Item::Instruction(next)) = self.items[index..]
                        .iter_mut()
                        .find(|item| matches!(item, Item::Instruction(_)))
                    && next.location.line < 0
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
            let location = blocks[index]
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
                        inline_small_exit: false,
                        preserve_inlined_jump_nop: false,
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
            let target_is_small_exit = blocks[target]
                .iter()
                .filter(|item| matches!(item, Item::Instruction(_)))
                .count()
                <= 4
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
                        inline_small_exit: false,
                        preserve_inlined_jump_nop: false,
                    }),
                );
            }
            if target_has_no_location {
                if let Some(Item::Instruction(instruction)) = copied
                    .iter_mut()
                    .find(|item| matches!(item, Item::Instruction(_)))
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
                    if instruction.location.line < 0 {
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
            if next_location.is_some_and(|location| location == instruction.location) {
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
                (STORE_FAST, LOAD_FAST_BORROW) => STORE_FAST_LOAD_FAST,
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
                inline_small_exit: first.inline_small_exit && second.inline_small_exit,
                preserve_inlined_jump_nop: false,
            }));
            index += 2;
        }
        self.items = fused;
    }

    fn exception_table(&self, extended_args: &[u8]) -> Result<Vec<u8>, CompileError> {
        let (_, labels) = self.positions(extended_args);
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
        let mut boundaries = regions
            .iter()
            .flat_map(|(start, end, _, _, _, _)| [*start, *end])
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
    fn resolves_forward_and_backward_jumps() {
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

        assert_eq!(assembler.finish().unwrap(), [128, 0, 77, 1, 77, 3]);
    }

    #[test]
    fn emits_extended_arguments() {
        let mut assembler = Assembler::default();
        assembler.emit(Opcode::new(82, 0), 0x1234);
        assert_eq!(assembler.finish().unwrap(), [69, 0x12, 82, 0x34]);
    }
}
