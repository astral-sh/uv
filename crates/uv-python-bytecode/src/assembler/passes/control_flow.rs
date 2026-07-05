use rustc_hash::{FxHashMap, FxHashSet};

use super::super::{
    CLEANUP_THROW, COMPARE_OP, CONTAINS_OP, COPY, END_ASYNC_FOR, END_SEND, IS_OP, Instruction,
    InstructionFlags, Item, JUMP_BACKWARD, JUMP_BACKWARD_NO_INTERRUPT, JUMP_FORWARD, LOAD_GLOBAL,
    NOP, NOT_TAKEN, Operand, POP_JUMP_IF_FALSE, POP_JUMP_IF_TRUE, POP_TOP, RAISE_VARARGS,
    STORE_FAST, TO_BOOL, UNARY_NOT, block_has_fallthrough, block_jump_target,
    cleanup_block_creation_order, ends_scope, is_unconditional_jump,
};
use crate::assembler::{Assembler, ExceptionRegion, Label, SourceLocation};
use crate::target::Opcode;
use crate::target::operands::ComparisonOperation;

impl Assembler {
    /// Replaces an overwritten local store with a stack pop.
    ///
    /// CPython applies this peephole before superinstruction fusion when two
    /// adjacent stores target the same local on the same traced line.
    pub(in crate::assembler) fn optimize_redundant_store_fast(&mut self) {
        for index in 0..self.items.len().saturating_sub(1) {
            let [Item::Instruction(first), Item::Instruction(second)] =
                &mut self.items[index..index + 2]
            else {
                continue;
            };
            if first.opcode == STORE_FAST
                && second.opcode == STORE_FAST
                && matches!(
                    (first.operand, second.operand),
                    (Operand::Value(first), Operand::Value(second)) if first == second
                )
                && first.location.line == second.location.line
            {
                first.opcode = POP_TOP;
                first.operand = Operand::Value(0);
            }
        }
    }

    pub(in crate::assembler) fn remove_unreachable_instructions(&mut self) -> Option<u32> {
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

    pub(in crate::assembler) fn reachable_items(&self) -> Vec<bool> {
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
                        || ends_scope(instruction.opcode);
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
        let mut label_blocks = FxHashMap::default();
        for (block, (start, end)) in block_ranges.iter().copied().enumerate() {
            for (offset, item_block) in item_blocks[start..end].iter_mut().enumerate() {
                let index = start + offset;
                *item_block = block;
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

    pub(in crate::assembler) fn optimize_boolean_conversions(&mut self) {
        for index in 0..self.items.len().saturating_sub(1) {
            let [Item::Instruction(current), Item::Instruction(next)] =
                &mut self.items[index..index + 2]
            else {
                continue;
            };
            match (current.opcode, next.opcode) {
                (TO_BOOL, TO_BOOL) => {
                    current.opcode = NOP;
                    current.operand = Operand::Value(0);
                    continue;
                }
                (UNARY_NOT, TO_BOOL) => {
                    current.opcode = NOP;
                    current.operand = Operand::Value(0);
                    next.opcode = UNARY_NOT;
                    next.operand = Operand::Value(0);
                    continue;
                }
                (UNARY_NOT, UNARY_NOT) => {
                    current.opcode = NOP;
                    current.operand = Operand::Value(0);
                    next.opcode = NOP;
                    next.operand = Operand::Value(0);
                    continue;
                }
                (CONTAINS_OP | IS_OP, UNARY_NOT) => {
                    let Operand::Value(argument) = current.operand else {
                        continue;
                    };
                    let opcode = current.opcode;
                    current.opcode = NOP;
                    current.operand = Operand::Value(0);
                    next.opcode = opcode;
                    next.operand = Operand::Value(argument ^ 1);
                    continue;
                }
                _ => {}
            }
            if next.opcode != TO_BOOL {
                continue;
            }
            let Operand::Value(argument) = current.operand else {
                continue;
            };
            let replacement = match current.opcode {
                COMPARE_OP => Some((COMPARE_OP, ComparisonOperation::force_boolean(argument))),
                CONTAINS_OP => Some((CONTAINS_OP, argument)),
                IS_OP => Some((IS_OP, argument)),
                _ => None,
            };
            let Some((opcode, argument)) = replacement else {
                continue;
            };
            current.opcode = NOP;
            current.operand = Operand::Value(0);
            next.opcode = opcode;
            next.operand = Operand::Value(argument);
        }
    }

    pub(in crate::assembler) fn thread_forward_jumps(&mut self) {
        #[derive(Clone, Copy)]
        struct ConditionalTarget {
            opcode: Opcode,
            operand: Operand,
            fallthrough_index: usize,
        }

        fn is_value_preserving_jump(items: &[Item], index: usize) -> bool {
            let Item::Instruction(instruction) = items[index] else {
                return false;
            };
            if !matches!(instruction.opcode, POP_JUMP_IF_FALSE | POP_JUMP_IF_TRUE)
                || !matches!(instruction.operand, Operand::Forward(_))
            {
                return false;
            }
            let mut previous = items[..index].iter().rev().filter_map(|item| {
                let Item::Instruction(instruction) = item else {
                    return None;
                };
                (instruction.opcode != NOP).then_some(instruction)
            });
            let Some(to_bool) = previous.next() else {
                return false;
            };
            let Some(copy) = previous.next() else {
                return false;
            };
            to_bool.opcode == TO_BOOL
                && copy.opcode == COPY
                && matches!(copy.operand, Operand::Value(1))
        }

        // CPython threads the value-preserving jumps used by boolean
        // expressions through a nested boolean test. If both tests jump on
        // the same truth value, they share the final target. Otherwise the
        // inner jump skips the redundant outer test and enters its POP_TOP
        // fallthrough path directly.
        let mut conditional_targets = FxHashMap::default();
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
                    (instruction.opcode != NOP).then_some((index + 1 + offset, *instruction))
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
            if copy.opcode != COPY
                || !matches!(copy.operand, Operand::Value(1))
                || to_bool.opcode != TO_BOOL
                || !matches!(jump.opcode, POP_JUMP_IF_FALSE | POP_JUMP_IF_TRUE)
                || !matches!(jump.operand, Operand::Forward(_))
                || not_taken.opcode != NOT_TAKEN
                || pop.opcode != POP_TOP
            {
                continue;
            }
            conditional_targets.insert(
                *label,
                ConditionalTarget {
                    opcode: jump.opcode,
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
                (instruction.opcode != target.opcode).then_some(target.fallthrough_index)
            })
            .collect::<Vec<_>>();
        fallthrough_indices.sort_unstable();
        fallthrough_indices.dedup();
        let fallthrough_labels = fallthrough_indices
            .iter()
            .map(|index| (*index, self.label()))
            .collect::<FxHashMap<_, _>>();
        for index in fallthrough_indices.into_iter().rev() {
            self.items
                .insert(index, Item::Label(fallthrough_labels[&index]));
        }

        let value_preserving_jumps = (0..self.items.len())
            .filter(|index| is_value_preserving_jump(&self.items, *index))
            .collect::<FxHashSet<_>>();
        for (index, item) in self.items.iter_mut().enumerate() {
            let Item::Instruction(instruction) = item else {
                continue;
            };
            if !value_preserving_jumps.contains(&index) {
                continue;
            }
            if !matches!(instruction.opcode, POP_JUMP_IF_FALSE | POP_JUMP_IF_TRUE) {
                continue;
            }
            let Operand::Forward(mut target) = instruction.operand else {
                continue;
            };
            for _ in 0..conditional_targets.len() {
                let Some(next) = conditional_targets.get(&target) else {
                    break;
                };
                if instruction.opcode == next.opcode {
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

        let mut jump_targets = FxHashMap::default();
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
            if instruction.has_flag(InstructionFlags::ALLOW_JUMP_THREADING_TARGET)
                && !instruction.has_flag(InstructionFlags::PRESERVE_INLINED_JUMP_NOP)
                && is_unconditional_jump(instruction.opcode)
                && let operand @ (Operand::Forward(_) | Operand::Backward(_)) = instruction.operand
            {
                jump_targets.insert(*label, (operand, instruction.opcode));
            }
        }

        for item in &mut self.items {
            let Item::Instruction(instruction) = item else {
                continue;
            };
            if instruction.opcode != JUMP_FORWARD
                || instruction.has_flag(InstructionFlags::PRESERVE_INLINED_JUMP_NOP)
            {
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
                    opcode = JUMP_BACKWARD;
                } else if next_opcode == JUMP_BACKWARD_NO_INTERRUPT {
                    opcode = JUMP_BACKWARD_NO_INTERRUPT;
                }
                target = next;
            }
            instruction.operand = operand;
            instruction.opcode = opcode;
        }
    }

    pub(in crate::assembler) fn remove_redundant_forward_jumps(&mut self) {
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
            if instruction.opcode != JUMP_FORWARD {
                index += 1;
                continue;
            }
            if instruction.has_flag(InstructionFlags::DEFER_REDUNDANT_JUMP_REMOVAL) {
                let Item::Instruction(instruction) = &mut self.items[index] else {
                    unreachable!();
                };
                instruction.remove_flag(InstructionFlags::DEFER_REDUNDANT_JUMP_REMOVAL);
                if instruction.has_flag(InstructionFlags::PRESERVE_NOP_AFTER_JUMP_THREADING) {
                    instruction.remove_flag(InstructionFlags::PRESERVE_NOP_AFTER_JUMP_THREADING);
                    instruction.insert_flag(InstructionFlags::PRESERVE_INLINED_JUMP_NOP);
                }
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
            if !instruction.has_flag(InstructionFlags::PRESERVE_INLINED_JUMP_NOP)
                && (instruction.location.line < 0
                    || previous_line == Some(instruction.location.line)
                    || next_line == Some(instruction.location.line))
            {
                self.items.remove(index);
                if instruction.has_flag(InstructionFlags::EXCLUDE_EXCEPTION_IF_EXTENDED)
                    && let Some(Item::Instruction(previous)) = self.items[..index]
                        .iter_mut()
                        .rev()
                        .find(|item| matches!(item, Item::Instruction(_)))
                {
                    previous.insert_flag(InstructionFlags::EXCLUDE_EXCEPTION_IF_EXTENDED);
                }
                if instruction.location.line >= 0
                    && let Some(Item::Instruction(next)) = self.items[index..]
                        .iter_mut()
                        .find(|item| matches!(item, Item::Instruction(_)))
                    && next.location.line < 0
                    && !next.has_flag(InstructionFlags::PRESERVE_NO_LOCATION)
                    && !next.has_flag(InstructionFlags::ALLOW_NO_LOCATION_BLOCK_INLINING)
                {
                    next.location = instruction.location;
                }
            } else {
                let Item::Instruction(instruction) = &mut self.items[index] else {
                    unreachable!();
                };
                instruction.opcode = NOP;
                instruction.operand = Operand::Value(0);
                index += 1;
            }
        }
    }

    pub(in crate::assembler) fn push_cold_blocks_to_end(&mut self) {
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
                    || ends_scope(instruction.opcode));
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

        let mut label_blocks = FxHashMap::default();
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
                            instruction.has_flag(InstructionFlags::EXCLUDE_EXCEPTION_IF_EXTENDED),
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
                    Item::Instruction(
                        Instruction::synthetic(
                            JUMP_BACKWARD_NO_INTERRUPT,
                            Operand::Backward(target),
                            location,
                            None,
                        )
                        .with_flag(
                            InstructionFlags::EXCLUDE_EXCEPTION_IF_EXTENDED,
                            exclude_exception_if_extended,
                        ),
                    ),
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

        // `codegen_add_yield_from` creates the `CLEANUP_THROW` block before it resumes
        // compiling the expression that follows the await. Our structured compiler creates the
        // same labels up front, but emits async-iterator cleanup blocks while unwinding nested
        // generators. Restore CPython's creation order when the cold blocks are collected.
        let cleanup_throw_positions = order
            .iter()
            .enumerate()
            .filter_map(|(position, old_index)| {
                (position + 1 < order.len()
                    && block_jump_target(&blocks[order[position + 1]]).is_some()
                    && blocks[*old_index]
                    .iter()
                    .any(|item| {
                        matches!(item, Item::Instruction(instruction) if instruction.opcode == CLEANUP_THROW)
                    }))
                    .then_some(position)
            })
            .collect::<Vec<_>>();
        if cleanup_throw_positions.windows(2).any(|positions| {
            cleanup_block_creation_order(&blocks[order[positions[0]]])
                > cleanup_block_creation_order(&blocks[order[positions[1]]])
        }) {
            let cleanup_throw_blocks = cleanup_throw_positions
                .iter()
                .map(|position| order[*position])
                .collect::<FxHashSet<_>>();
            // Move each cleanup and its continuation as a pair. Intervening cold handler blocks
            // retain their relative position, as they do in CPython's block list.
            loop {
                let ordered_cleanup_blocks = order
                    .iter()
                    .enumerate()
                    .filter(|(_, old_index)| cleanup_throw_blocks.contains(old_index))
                    .collect::<Vec<_>>();
                let Some(inversion) = ordered_cleanup_blocks.windows(2).find(|blocks_in_order| {
                    cleanup_block_creation_order(&blocks[*blocks_in_order[0].1])
                        > cleanup_block_creation_order(&blocks[*blocks_in_order[1].1])
                }) else {
                    break;
                };
                let insertion_position = inversion[0].0;
                let group_position = inversion[1].0;
                let group = order
                    .drain(group_position..group_position + 2)
                    .collect::<Vec<_>>();
                order.splice(insertion_position..insertion_position, group);
            }

            // The outer inlined comprehension's successful restore was compiled after its
            // exceptional restore in our unwind order. CPython compiled it before the nested
            // handlers, so collecting those handlers also makes the terminal restore the
            // fallthrough from `END_ASYNC_FOR` and eliminates the synthetic forward jump.
            let terminal_restore = (0..order.len().saturating_sub(1)).find_map(|position| {
                let target = blocks[order[position]]
                    .iter()
                    .any(|item| {
                        matches!(item, Item::Instruction(instruction) if instruction.opcode == END_ASYNC_FOR)
                    })
                    .then(|| block_jump_target(&blocks[order[position + 1]]))
                    .flatten()
                    .and_then(|target| label_blocks.get(&target).copied())?;
                let start = order.iter().position(|old_index| *old_index == target)?;
                let mut end = start;
                while end + 1 < order.len() && block_has_fallthrough(&blocks[order[end]]) {
                    end += 1;
                }
                let exits_scope = blocks[order[end]].iter().rev().find_map(|item| {
                    if let Item::Instruction(instruction) = item {
                        Some(ends_scope(instruction.opcode))
                    } else {
                        None
                    }
                });
                (exits_scope == Some(true)).then_some((position + 2, start, end))
            });
            if let Some((insertion_position, start, end)) = terminal_restore {
                let terminal_restore = order.drain(start..=end).collect::<Vec<_>>();
                order.splice(insertion_position..insertion_position, terminal_restore);
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

        let mut label_positions = FxHashMap::default();
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
                    if instruction.opcode == JUMP_FORWARD {
                        instruction.opcode = JUMP_BACKWARD_NO_INTERRUPT;
                    }
                } else {
                    instruction.operand = Operand::Forward(target);
                    if matches!(
                        instruction.opcode,
                        JUMP_BACKWARD | JUMP_BACKWARD_NO_INTERRUPT
                    ) {
                        instruction.opcode = JUMP_FORWARD;
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

    pub(in crate::assembler) fn duplicate_exit_blocks(&mut self) {
        let mut blocks = Vec::<Vec<Item>>::new();
        let mut block = Vec::new();
        for item in std::mem::take(&mut self.items) {
            if matches!(item, Item::Label(label) if
                block.iter().any(|item| matches!(item, Item::Instruction(_)))
                    || (!block.is_empty() && self.preserved_block_boundaries.contains(&label)))
            {
                blocks.push(std::mem::take(&mut block));
            }
            let ends_block = matches!(item, Item::Instruction(instruction) if
                !matches!(instruction.operand, Operand::Value(_))
                    || ends_scope(instruction.opcode));
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
            .collect::<FxHashMap<_, _>>();
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

        let exception_split_labels = self
            .exception_regions
            .iter()
            .flat_map(|region| [region.start, region.end])
            .collect::<FxHashSet<_>>();
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
                            Some(ends_scope(instruction.opcode))
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
        let mut copied_exception_regions = Vec::new();
        let mut drop_block = vec![false; blocks.len()];
        let mut remaining_predecessors = predecessors.clone();
        let mut sources = (0..blocks.len()).collect::<Vec<_>>();
        let mut source_cursor = 0;
        while let Some(&source) = sources.get(source_cursor) {
            source_cursor += 1;
            let Some(target_label) = block_jump_target(&blocks[source]) else {
                continue;
            };
            let Some(target) = label_blocks.get(&target_label).copied() else {
                continue;
            };
            let source_has_fallthrough = block_has_fallthrough(&blocks[source]);
            let mut target_chain_end = target;
            while target_chain_end + 1 < blocks.len()
                && block_has_fallthrough(&blocks[target_chain_end])
                && predecessors[target_chain_end + 1] == 1
                && blocks[target_chain_end + 1].iter().any(|item| {
                    matches!(item, Item::Label(label) if exception_split_labels.contains(label))
                })
            {
                target_chain_end += 1;
            }
            let target_blocks = &blocks[target..=target_chain_end];
            let target_pre_fusion_size = target_blocks
                .iter()
                .flatten()
                .map(|item| match item {
                    Item::Instruction(instruction) => {
                        let fused_push_null = instruction.opcode == LOAD_GLOBAL
                            && matches!(instruction.operand, Operand::Value(argument) if argument & 1 != 0);
                        1 + usize::from(fused_push_null)
                    }
                    Item::Label(label) => usize::from(
                        target_chain_end > target && exception_split_labels.contains(label),
                    ),
                })
                .sum::<usize>();
            let target_terminal_opcode = target_blocks
                .iter()
                .rev()
                .flat_map(|block| block.iter().rev())
                .find_map(|item| {
                    if let Item::Instruction(instruction) = item {
                        Some(instruction.opcode)
                    } else {
                        None
                    }
                });
            let target_is_small_exit = target_pre_fusion_size <= 4
                && target_terminal_opcode.is_some_and(|opcode| {
                    ends_scope(opcode)
                        // An exception-boundary split before a raise represents CPython's
                        // `SETUP_FINALLY` inside one basic block. Return paths can also contain
                        // optimized-away `POP_BLOCK` instructions that are not recoverable from
                        // the final region labels, so keep their existing block boundary.
                        && (target_chain_end == target || opcode == RAISE_VARARGS)
                });
            let source_allows_small_exit_inlining = blocks[source]
                .iter()
                .rev()
                .find_map(|item| {
                    if let Item::Instruction(instruction) = item {
                        Some(instruction.has_flag(InstructionFlags::INLINE_SMALL_EXIT))
                    } else {
                        None
                    }
                })
                .unwrap_or(false);
            let target_contains_end_send = target_blocks.iter().flatten().any(|item| {
                matches!(item, Item::Instruction(instruction) if instruction.opcode == END_SEND)
            });
            let target_has_no_location = target_blocks.iter().flatten().all(|item| {
                !matches!(item, Item::Instruction(instruction) if instruction.location.line >= 0)
            });
            let target_allows_no_location_block_inlining = blocks[target]
                .iter()
                .rev()
                .find_map(|item| match item {
                    Item::Instruction(instruction) => Some(
                        instruction.has_flag(InstructionFlags::ALLOW_NO_LOCATION_BLOCK_INLINING),
                    ),
                    Item::Label(_) => None,
                })
                .unwrap_or(false);
            let target_is_no_location_no_fallthrough = target_has_no_location
                && target_allows_no_location_block_inlining
                && !block_has_fallthrough(&blocks[target]);
            let inline_small_exit = !source_has_fallthrough
                && target_is_small_exit
                && source_allows_small_exit_inlining
                && !target_contains_end_send
                && !exception_handler_blocks[target];
            let inline_no_location_block =
                !source_has_fallthrough && target_is_no_location_no_fallthrough;
            let later_jump_predecessor = (source + 1..blocks.len()).any(|predecessor| {
                block_jump_target(&blocks[predecessor])
                    .and_then(|label| label_blocks.get(&label).copied())
                    == Some(target)
            });
            if exit_without_unique_predecessor[target]
                && target_pre_fusion_size > 4
                && remaining_predecessors[target] > 1
                && later_jump_predecessor
            {
                // CPython's second exit-duplication pass runs after cold blocks move. For a
                // large class footer shared by warm and cold jumps, that makes the cold, later
                // predecessor receive the copy while the warm predecessor keeps the original.
                continue;
            }
            if !(inline_small_exit || inline_no_location_block)
                && (!(exit_without_unique_predecessor[target]
                    || target_is_no_location_no_fallthrough)
                    || remaining_predecessors[target] <= 1)
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
            let mut copied = target_blocks
                .iter()
                .flat_map(|block| block.iter().copied())
                .collect::<Vec<_>>();
            let preserve_inlined_jump_nop = if inline_small_exit && source_location.line >= 0 {
                blocks[source]
                    .iter()
                    .rev()
                    .find_map(|item| {
                        if let Item::Instruction(instruction) = item {
                            Some(
                                instruction.has_flag(InstructionFlags::PRESERVE_INLINED_JUMP_NOP)
                                    || matches!(
                                        (
                                            instruction.preserve_direct_inlined_jump_nop,
                                            instruction.operand,
                                        ),
                                        (Some(expected), Operand::Forward(actual)) if expected == actual
                                    ),
                            )
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
            let mut copied_labels = FxHashMap::default();
            for item in &mut copied {
                if let Item::Label(label) = item {
                    let original = *label;
                    if !replaced_entry_label {
                        *label = copied_label;
                        replaced_entry_label = true;
                    } else {
                        *label = self.label();
                    }
                    copied_labels.insert(original, *label);
                }
            }
            if !replaced_entry_label {
                copied.insert(0, Item::Label(copied_label));
            }
            let trailing_region_ends = self
                .exception_regions
                .iter()
                .filter(|region| {
                    copied_labels.contains_key(&region.start)
                        && !copied_labels.contains_key(&region.end)
                })
                .map(|region| region.end)
                .collect::<Vec<_>>();
            for original in trailing_region_ends {
                let copied_end = self.label();
                copied.push(Item::Label(copied_end));
                copied_labels.insert(original, copied_end);
            }
            copied_exception_regions.extend(self.exception_regions.iter().filter_map(|region| {
                Some(ExceptionRegion {
                    start: *copied_labels.get(&region.start)?,
                    end: *copied_labels.get(&region.end)?,
                    target: copied_labels
                        .get(&region.target)
                        .copied()
                        .unwrap_or(region.target),
                    depth: region.depth,
                    preserve_lasti: region.preserve_lasti,
                })
            }));
            if preserve_inlined_jump_nop {
                let position = copied
                    .iter()
                    .position(|item| matches!(item, Item::Instruction(_)))
                    .unwrap_or(copied.len());
                copied.insert(
                    position,
                    Item::Instruction(Instruction::synthetic(
                        NOP,
                        Operand::Value(0),
                        source_location,
                        None,
                    )),
                );
            }
            if target_has_no_location && !(inline_no_location_block && predecessors[source] > 1) {
                if let Some(Item::Instruction(instruction)) = copied
                    .iter_mut()
                    .find(|item| matches!(item, Item::Instruction(_)))
                    && !instruction.has_flag(InstructionFlags::PRESERVE_NO_LOCATION)
                {
                    instruction.location = source_location;
                }
            }
            if inline_small_exit || inline_no_location_block {
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
                                Some((position, instruction.opcode))
                            } else {
                                None
                            }
                        })
                        .nth(1)
                        .filter(|(_, opcode)| *opcode == NOP)
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
            if inline_small_exit || inline_no_location_block {
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
            if inline_no_location_block {
                blocks[source].extend(copied);
                // The copied jump can turn this source into another eligible unlocated block.
                // Revisit earlier jump predecessors until the chain reaches a fixed point.
                for (predecessor, block) in blocks.iter().enumerate().take(source) {
                    if block_jump_target(block).and_then(|label| label_blocks.get(&label).copied())
                        == Some(source)
                    {
                        sources.push(predecessor);
                    }
                }
            } else if source_has_fallthrough {
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
        self.exception_regions.extend(copied_exception_regions);

        for target in 0..blocks.len() {
            if exception_handler_blocks[target] {
                continue;
            }
            if remaining_predecessors[target] == 1
                && let Some(Item::Instruction(first)) = blocks[target]
                    .iter()
                    .find(|item| matches!(item, Item::Instruction(_)))
                && first.opcode == NOP
                && first.has_flag(InstructionFlags::CONVERTED_POP_BLOCK)
                && first.location.line < 0
                && !first.has_flag(InstructionFlags::PRESERVE_NO_LOCATION)
                && let Some(location) = blocks.iter().find_map(|block| {
                    block_jump_target(block)
                        .and_then(|label| label_blocks.get(&label).copied())
                        .filter(|index| *index == target)
                        .and_then(|_| {
                            block.iter().rev().find_map(|item| {
                                if let Item::Instruction(instruction) = item {
                                    Some(instruction.location)
                                } else {
                                    None
                                }
                            })
                        })
                })
                && let Some(Item::Instruction(first)) = blocks[target]
                    .iter_mut()
                    .find(|item| matches!(item, Item::Instruction(_)))
            {
                // A converted `POP_BLOCK` inherits the line of its sole jump predecessor.
                // This remains observable when another predecessor copied the small exit.
                first.location = location;
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
                && !instruction.has_flag(InstructionFlags::PRESERVE_NO_LOCATION)
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

    pub(in crate::assembler) fn propagate_locations_within_blocks(&mut self) {
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
                    if instruction.location.line < 0
                        && !instruction.has_flag(InstructionFlags::PRESERVE_NO_LOCATION)
                    {
                        if let Some(location) = previous {
                            instruction.location = location;
                        }
                    } else {
                        previous = Some(instruction.location);
                    }
                    if !matches!(instruction.operand, Operand::Value(_))
                        || ends_scope(instruction.opcode)
                    {
                        previous = None;
                        block_has_instruction = false;
                    }
                }
            }
        }
    }

    pub(in crate::assembler) fn remove_redundant_nops(&mut self) {
        let mut index = 0;
        while index < self.items.len() {
            let Item::Instruction(instruction) = self.items[index] else {
                index += 1;
                continue;
            };
            if instruction.opcode != NOP {
                index += 1;
                continue;
            }
            if instruction.location.line < 0 {
                self.items.remove(index);
                continue;
            }
            if instruction.has_flag(InstructionFlags::CONVERTED_POP_BLOCK)
                && let Some(previous) = self.items[..index].iter().rev().find_map(|item| {
                    if let Item::Instruction(instruction) = item {
                        Some(instruction)
                    } else {
                        None
                    }
                })
                && previous.location.line == instruction.location.line
                && !(ends_scope(previous.opcode) || is_unconditional_jump(previous.opcode))
            {
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
}
