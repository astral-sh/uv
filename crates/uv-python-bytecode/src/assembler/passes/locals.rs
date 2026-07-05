use rustc_hash::{FxHashMap, FxHashSet};

use super::super::{
    CHECK_EXC_MATCH, COMPARE_OP, COPY, DELETE_DEREF, DELETE_FAST, DICT_MERGE, DICT_UPDATE,
    END_SEND, FOR_ITER, FORMAT_SIMPLE, GET_ANEXT, GET_LEN, GET_YIELD_FROM_ITER, IMPORT_FROM,
    Instruction, InstructionFlags, Item, LIST_APPEND, LIST_EXTEND, LOAD_ATTR, LOAD_COMMON_CONSTANT,
    LOAD_CONST, LOAD_FAST, LOAD_FAST_AND_CLEAR, LOAD_FAST_BORROW,
    LOAD_FAST_BORROW_LOAD_FAST_BORROW, LOAD_FAST_CHECK, LOAD_FAST_LOAD_FAST, LOAD_SMALL_INT,
    LOAD_SPECIAL, LOAD_SUPER_ATTR, MAP_ADD, MATCH_KEYS, MATCH_MAPPING, MATCH_SEQUENCE, NOP,
    Operand, POP_EXCEPT, POP_TOP, PUSH_EXC_INFO, RERAISE, RETURN_VALUE, SEND, SET_ADD,
    SET_FUNCTION_ATTRIBUTE, SET_UPDATE, STORE_DEREF, STORE_FAST, STORE_FAST_LOAD_FAST,
    STORE_FAST_STORE_FAST, STORE_GLOBAL, SWAP, WITH_EXCEPT_START, block_has_fallthrough,
    block_jump_target, ends_scope, opcode_num_popped, opcode_num_pushed, opcode_stack_effect,
};
use crate::assembler::Assembler;

impl Assembler {
    /// Converts potentially uninitialized local loads to checked loads.
    ///
    /// Mirrors CPython's `add_checks_for_loads_of_uninitialized_variables`
    /// flow-graph pass. A set bit means that a local may be uninitialized on
    /// at least one path into the block.
    pub(in crate::assembler) fn add_checks_for_uninitialized_loads(
        &mut self,
        local_count: usize,
        parameter_count: usize,
    ) {
        if local_count == 0 {
            return;
        }

        let mut block_labels = FxHashSet::default();
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
            if !matches!(instruction.operand, Operand::Value(_)) || ends_scope(instruction.opcode) {
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
        let mut label_blocks = FxHashMap::default();
        let mut label_positions = FxHashMap::default();
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

        let mut exception_successors = FxHashMap::default();
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
        for unsafe_local in unsafe_at_entry[0]
            .iter_mut()
            .take(local_count)
            .skip(parameter_count.min(local_count))
        {
            *unsafe_local = true;
        }
        let mut needs_check = FxHashSet::default();
        let mut checked_loads_needing_check = FxHashSet::default();
        let mut pending = (0..blocks.len()).collect::<Vec<_>>();
        let mut queued = vec![true; blocks.len()];

        while let Some(block_index) = pending.pop() {
            queued[block_index] = false;
            let mut unsafe_locals = unsafe_at_entry[block_index].clone();
            for unsafe_local in unsafe_locals.iter_mut().take(local_count).skip(64) {
                *unsafe_local = true;
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
                match instruction.opcode {
                    DELETE_FAST | LOAD_FAST_AND_CLEAR => unsafe_locals[local] = true,
                    STORE_FAST => unsafe_locals[local] = false,
                    LOAD_FAST_CHECK => {
                        if unsafe_locals[local] {
                            checked_loads_needing_check.insert(*index);
                        }
                        unsafe_locals[local] = false;
                    }
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

        for (index, item) in self.items.iter_mut().enumerate() {
            let Item::Instruction(instruction) = item else {
                continue;
            };
            if instruction.opcode == LOAD_FAST_CHECK
                && !checked_loads_needing_check.contains(&index)
            {
                instruction.opcode = LOAD_FAST;
            }
        }
        for index in needs_check {
            let Item::Instruction(instruction) = &mut self.items[index] else {
                unreachable!();
            };
            if instruction.opcode == LOAD_FAST {
                instruction.opcode = LOAD_FAST_CHECK;
            }
        }
    }

    /// Removes repeated checked loads within a basic block.
    ///
    /// Once a checked load succeeds, CPython's definite-assignment scan treats
    /// that local as initialized for subsequent instructions on the same path.
    pub(in crate::assembler) fn remove_redundant_checked_loads(&mut self) {
        let mut initialized = FxHashSet::default();
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
            match instruction.opcode {
                DELETE_DEREF | DELETE_FAST | LOAD_FAST_AND_CLEAR => {
                    initialized.remove(&argument);
                }
                LOAD_FAST_CHECK => {
                    if initialized.contains(&argument) {
                        instruction.opcode = LOAD_FAST;
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
            if ends_scope(instruction.opcode) {
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
    pub(in crate::assembler) fn optimize_load_fast(&mut self) {
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

        fn kill_local(unsafe_loads: &mut FxHashSet<usize>, stack: &[Reference], local: u32) {
            for reference in stack {
                if reference.local == Some(local)
                    && let Some(producer) = reference.producer
                {
                    unsafe_loads.insert(producer);
                }
            }
        }

        fn store_local(
            unsafe_loads: &mut FxHashSet<usize>,
            stack: &[Reference],
            local: u32,
            reference: Reference,
        ) {
            kill_local(unsafe_loads, stack, local);
            if let Some(producer) = reference.producer {
                unsafe_loads.insert(producer);
            }
        }

        if !self.items.iter().any(|item| {
            matches!(
                item,
                Item::Instruction(instruction)
                    if matches!(instruction.opcode, LOAD_FAST | LOAD_FAST_LOAD_FAST)
            )
        }) {
            return;
        }

        let mut block_labels = FxHashSet::default();
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
            if (matches!(item, Item::Label(label) if block_labels.contains(label) || self.preserved_block_boundaries.contains(label))
                || matches!(item, Item::Instruction(instruction) if instruction.has_flag(InstructionFlags::BORROW_UNREACHABLE_ENTRY)))
                && !block.is_empty()
            {
                blocks.push(std::mem::take(&mut block));
            }
            let Item::Instruction(instruction) = item else {
                continue;
            };
            block.push(index);
            if !matches!(instruction.operand, Operand::Value(_))
                || ends_scope(instruction.opcode)
                || (instruction.opcode == NOP
                    && instruction.has_flag(InstructionFlags::PRESERVE_INLINED_JUMP_NOP))
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
        let mut label_blocks = FxHashMap::default();
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
        let mut borrow_unreachable_blocks = self
            .borrow_unreachable_blocks
            .iter()
            .filter_map(|label| label_blocks.get(label).copied())
            .collect::<FxHashSet<_>>();
        borrow_unreachable_blocks.extend(self.items.iter().enumerate().filter_map(
            |(index, item)| match item {
                Item::Instruction(instruction)
                    if instruction.has_flag(InstructionFlags::BORROW_UNREACHABLE_ENTRY) =>
                {
                    item_blocks[index]
                }
                Item::Instruction(_) | Item::Label(_) => None,
            },
        ));
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
                last.opcode == NOP && last.has_flag(InstructionFlags::PRESERVE_INLINED_JUMP_NOP);
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
                    cumulative_effect += opcode_stack_effect(instruction.opcode, 0);
                    if let Some(depth_after) = instruction.depth_after {
                        start_depth = Some(i64::from(depth_after) - cumulative_effect);
                        break;
                    }
                    continue;
                };
                cumulative_effect += opcode_stack_effect(instruction.opcode, argument);
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
            let mut unsafe_loads = FxHashSet::default();

            for index in &block {
                let Item::Instruction(instruction) = self.items[*index] else {
                    unreachable!();
                };
                let argument = match instruction.operand {
                    Operand::Value(argument) => argument,
                    Operand::Forward(_) | Operand::Backward(_) => 0,
                };
                match instruction.opcode {
                    DELETE_FAST => kill_local(&mut unsafe_loads, &stack, argument),
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
                    COPY => {
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
                    SWAP => {
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
                    FORMAT_SIMPLE | GET_ANEXT | GET_LEN | GET_YIELD_FROM_ITER | MATCH_KEYS
                    | MATCH_MAPPING | MATCH_SEQUENCE | WITH_EXCEPT_START | IMPORT_FROM => {
                        let popped = opcode_num_popped(instruction.opcode, argument);
                        let pushed = opcode_num_pushed(instruction.opcode, argument);
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
                    DICT_MERGE | DICT_UPDATE | LIST_APPEND | LIST_EXTEND | MAP_ADD | RERAISE
                    | SET_ADD | SET_UPDATE => {
                        let popped = opcode_num_popped(instruction.opcode, argument);
                        let pushed = opcode_num_pushed(instruction.opcode, argument);
                        for _ in 0..popped.saturating_sub(pushed) {
                            pop_reference(&mut stack);
                        }
                    }
                    END_SEND | SET_FUNCTION_ATTRIBUTE => {
                        let top = pop_reference(&mut stack);
                        pop_reference(&mut stack);
                        stack.push(top);
                    }
                    CHECK_EXC_MATCH => {
                        pop_reference(&mut stack);
                        stack.push(Reference {
                            producer: None,
                            local: None,
                        });
                    }
                    FOR_ITER => stack.push(Reference {
                        producer: None,
                        local: None,
                    }),
                    LOAD_ATTR | LOAD_SUPER_ATTR => {
                        let receiver = pop_reference(&mut stack);
                        if instruction.opcode == LOAD_SUPER_ATTR {
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
                    PUSH_EXC_INFO | LOAD_SPECIAL => {
                        let top = pop_reference(&mut stack);
                        stack.push(Reference {
                            producer: None,
                            local: None,
                        });
                        stack.push(top);
                    }
                    SEND => {
                        pop_reference(&mut stack);
                        stack.push(Reference {
                            producer: None,
                            local: None,
                        });
                    }
                    _ => {
                        for _ in 0..opcode_num_popped(instruction.opcode, argument) {
                            pop_reference(&mut stack);
                        }
                        for _ in 0..opcode_num_pushed(instruction.opcode, argument) {
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
                    Item::Instruction(instruction) => (
                        instruction.has_flag(InstructionFlags::FORCE_OWNED_LOAD),
                        instruction.has_flag(InstructionFlags::STRICT_OWNED_LOAD),
                    ),
                    Item::Label(_) => unreachable!(),
                };
                if strict_owned_load {
                    continue;
                }
                if force_owned_load {
                    let following = block[position + 1..]
                        .iter()
                        .filter_map(|index| match self.items[*index] {
                            Item::Instruction(instruction) if instruction.opcode != NOP => {
                                Some(instruction.opcode)
                            }
                            _ => None,
                        })
                        .take(2)
                        .collect::<Vec<_>>();
                    let directly_consumed = following.first().is_some_and(|opcode| {
                        matches!(*opcode, RETURN_VALUE | STORE_DEREF | STORE_GLOBAL)
                    }) || matches!(following.as_slice(), [POP_TOP, next] if *next != POP_EXCEPT)
                        || matches!(
                            following.as_slice(),
                            [
                                LOAD_COMMON_CONSTANT | LOAD_CONST | LOAD_SMALL_INT,
                                COMPARE_OP
                            ]
                        );
                    if !directly_consumed {
                        continue;
                    }
                }
                let Item::Instruction(instruction) = &mut self.items[index] else {
                    unreachable!();
                };
                match instruction.opcode {
                    LOAD_FAST => {
                        instruction.opcode = LOAD_FAST_BORROW;
                    }
                    LOAD_FAST_LOAD_FAST => {
                        instruction.opcode = LOAD_FAST_BORROW_LOAD_FAST_BORROW;
                    }
                    _ => {}
                }
            }
        }
    }

    pub(in crate::assembler) fn fuse_superinstructions(&mut self) {
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
            if first.has_flag(InstructionFlags::PREVENT_FUSION_WITH_NEXT)
                || second.has_flag(InstructionFlags::PREVENT_FUSION_WITH_PREVIOUS)
                || second.has_flag(InstructionFlags::BORROW_UNREACHABLE_ENTRY)
            {
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
            let opcode = match (first.opcode, second.opcode) {
                (LOAD_FAST_BORROW, LOAD_FAST_BORROW) => LOAD_FAST_BORROW_LOAD_FAST_BORROW,
                (LOAD_FAST, LOAD_FAST) => LOAD_FAST_LOAD_FAST,
                (STORE_FAST, LOAD_FAST | LOAD_FAST_BORROW) => STORE_FAST_LOAD_FAST,
                (STORE_FAST, STORE_FAST) => STORE_FAST_STORE_FAST,
                _ => {
                    fused.push(self.items[index]);
                    index += 1;
                    continue;
                }
            };
            fused.push(Item::Instruction(Instruction::fused(
                opcode,
                Operand::Value((first_argument << 4) | second_argument),
                first,
                second,
            )));
            index += 2;
        }
        self.items = fused;
    }
}
