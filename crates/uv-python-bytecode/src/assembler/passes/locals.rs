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
use crate::assembler::block_graph::ItemPosition;

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

        let graph = self
            .graph
            .as_ref()
            .expect("assembler pass requires the block graph");
        let order = graph.order().to_vec();
        if order.is_empty() {
            return;
        }
        let block_indices = order
            .iter()
            .copied()
            .enumerate()
            .map(|(index, block)| (block, index))
            .collect::<FxHashMap<_, _>>();
        let positions = graph.item_positions();
        let linear_indices = positions
            .iter()
            .copied()
            .enumerate()
            .map(|(index, position)| (position, index))
            .collect::<FxHashMap<_, _>>();
        let label_positions = positions
            .iter()
            .enumerate()
            .filter_map(|(index, position)| match graph.item(*position) {
                Item::Label(label) => Some((*label, index)),
                Item::Instruction(_) => None,
            })
            .collect::<FxHashMap<_, _>>();

        let mut exception_successors = FxHashMap::default();
        for position in positions.iter().copied() {
            if !matches!(graph.item(position), Item::Instruction(_)) {
                continue;
            }
            let index = linear_indices[&position];
            let mut innermost = None::<(usize, usize, usize)>;
            for region in &self.exception_regions {
                let (Some(&start), Some(&end), Some(target)) = (
                    label_positions.get(&region.start),
                    label_positions.get(&region.end),
                    graph
                        .label_block(region.target)
                        .and_then(|block| block_indices.get(&block).copied()),
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
                exception_successors.insert(position, target);
            }
        }

        let mut unsafe_at_entry = vec![vec![false; local_count]; order.len()];
        for unsafe_local in unsafe_at_entry[0]
            .iter_mut()
            .take(local_count)
            .skip(parameter_count.min(local_count))
        {
            *unsafe_local = true;
        }
        let mut needs_check = FxHashSet::default();
        let mut checked_loads_needing_check = FxHashSet::default();
        let mut pending = (0..order.len()).collect::<Vec<_>>();
        let mut queued = vec![true; order.len()];

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

            let block = order[block_index];
            for position in graph.block_item_positions(block) {
                let Item::Instruction(instruction) = graph.item(position) else {
                    continue;
                };
                if let Some(target) = exception_successors.get(&position).copied() {
                    propagate(target, &unsafe_locals);
                }
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
                            checked_loads_needing_check.insert(position);
                        }
                        unsafe_locals[local] = false;
                    }
                    LOAD_FAST => {
                        if unsafe_locals[local] {
                            needs_check.insert(position);
                        }
                        unsafe_locals[local] = false;
                    }
                    _ => {}
                }
            }

            if block_has_fallthrough(graph[block].items()) && block_index + 1 < order.len() {
                propagate(block_index + 1, &unsafe_locals);
            }
            if let Some(target) = block_jump_target(graph[block].items())
                && let Some(target) = graph
                    .label_block(target)
                    .and_then(|block| block_indices.get(&block).copied())
            {
                propagate(target, &unsafe_locals);
            }
        }

        let graph = self
            .graph
            .as_mut()
            .expect("assembler pass requires the block graph");
        for position in positions {
            let Item::Instruction(instruction) = graph.item_mut(position) else {
                continue;
            };
            if instruction.opcode == LOAD_FAST_CHECK
                && !checked_loads_needing_check.contains(&position)
            {
                instruction.opcode = LOAD_FAST;
            }
        }
        for position in needs_check {
            let Item::Instruction(instruction) = graph.item_mut(position) else {
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
        let graph = self
            .graph
            .as_mut()
            .expect("assembler pass requires the block graph");
        let order = graph.order().to_vec();
        for block in order {
            let mut initialized = FxHashSet::default();
            for item in graph[block].items_mut() {
                let Item::Instruction(instruction) = item else {
                    continue;
                };
                let Operand::Value(argument) = instruction.operand else {
                    break;
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
                    break;
                }
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
            producer: Option<ItemPosition>,
            local: Option<u32>,
        }

        fn pop_reference(stack: &mut Vec<Reference>) -> Reference {
            stack.pop().unwrap_or(Reference {
                producer: None,
                local: None,
            })
        }

        fn kill_local(unsafe_loads: &mut FxHashSet<ItemPosition>, stack: &[Reference], local: u32) {
            for reference in stack {
                if reference.local == Some(local)
                    && let Some(producer) = reference.producer
                {
                    unsafe_loads.insert(producer);
                }
            }
        }

        fn store_local(
            unsafe_loads: &mut FxHashSet<ItemPosition>,
            stack: &[Reference],
            local: u32,
            reference: Reference,
        ) {
            kill_local(unsafe_loads, stack, local);
            if let Some(producer) = reference.producer {
                unsafe_loads.insert(producer);
            }
        }

        let graph = self
            .graph
            .as_ref()
            .expect("assembler pass requires the block graph");
        if !graph.iter_items().any(|item| {
            matches!(
                item,
                Item::Instruction(instruction)
                    if matches!(instruction.opcode, LOAD_FAST | LOAD_FAST_LOAD_FAST)
            )
        }) {
            return;
        }

        let mut block_labels = FxHashSet::default();
        for item in graph.iter_items() {
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
        block_labels.extend(self.preserved_block_boundaries.iter().copied());

        let graph = self
            .graph
            .as_mut()
            .expect("assembler pass requires the block graph");
        graph.repartition(
            &block_labels,
            |instruction| instruction.has_flag(InstructionFlags::BORROW_UNREACHABLE_ENTRY),
            |instruction| {
                !matches!(instruction.operand, Operand::Value(_))
                    || ends_scope(instruction.opcode)
                    || (instruction.opcode == NOP
                        && instruction.has_flag(InstructionFlags::PRESERVE_INLINED_JUMP_NOP))
            },
        );
        let blocks = graph.order().to_vec();
        let mut borrow_unreachable_blocks = self
            .borrow_unreachable_blocks
            .iter()
            .filter_map(|label| graph.label_block(*label))
            .collect::<FxHashSet<_>>();
        borrow_unreachable_blocks.extend(blocks.iter().copied().filter(|block| {
            graph[*block].items().iter().any(|item| {
                matches!(item, Item::Instruction(instruction) if
                    instruction.has_flag(InstructionFlags::BORROW_UNREACHABLE_ENTRY))
            })
        }));
        let block_positions = blocks
            .iter()
            .copied()
            .enumerate()
            .map(|(position, block)| (block, position))
            .collect::<FxHashMap<_, _>>();
        let mut reachable = FxHashSet::default();
        let mut pending = blocks.first().copied().into_iter().collect::<Vec<_>>();
        while let Some(block) = pending.pop() {
            if borrow_unreachable_blocks.contains(&block) {
                continue;
            }
            if !reachable.insert(block) {
                continue;
            }
            let Some(last) = graph[block].last_instruction() else {
                continue;
            };
            let folded_jump_has_no_fallthrough =
                last.opcode == NOP && last.has_flag(InstructionFlags::PRESERVE_INLINED_JUMP_NOP);
            if !folded_jump_has_no_fallthrough
                && block_has_fallthrough(&[Item::Instruction(*last)])
                && let Some(next) = blocks.get(block_positions[&block] + 1)
            {
                pending.push(*next);
            }
            if let Operand::Forward(label) | Operand::Backward(label) = last.operand
                && let Some(target) = graph.label_block(label)
            {
                pending.push(target);
            }
        }

        for block in blocks {
            if !reachable.contains(&block) {
                continue;
            }
            let instruction_positions = graph
                .block_item_positions(block)
                .into_iter()
                .filter(|position| matches!(graph.item(*position), Item::Instruction(_)))
                .collect::<Vec<_>>();
            let mut cumulative_effect = 0_i64;
            let mut start_depth = None;
            for position in &instruction_positions {
                let Item::Instruction(instruction) = *graph.item(*position) else {
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

            for position in &instruction_positions {
                let Item::Instruction(instruction) = *graph.item(*position) else {
                    unreachable!();
                };
                let argument = match instruction.operand {
                    Operand::Value(argument) => argument,
                    Operand::Forward(_) | Operand::Backward(_) => 0,
                };
                match instruction.opcode {
                    DELETE_FAST => kill_local(&mut unsafe_loads, &stack, argument),
                    LOAD_FAST => stack.push(Reference {
                        producer: Some(*position),
                        local: Some(argument),
                    }),
                    LOAD_FAST_AND_CLEAR => {
                        kill_local(&mut unsafe_loads, &stack, argument);
                        stack.push(Reference {
                            producer: Some(*position),
                            local: Some(argument),
                        });
                    }
                    LOAD_FAST_LOAD_FAST => {
                        stack.push(Reference {
                            producer: Some(*position),
                            local: Some(argument >> 4),
                        });
                        stack.push(Reference {
                            producer: Some(*position),
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
                            producer: Some(*position),
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
                        for producer in instruction_positions
                            .iter()
                            .copied()
                            .take(pushed.saturating_sub(popped))
                        {
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
            for (index, position) in instruction_positions.iter().copied().enumerate() {
                if unsafe_loads.contains(&position) {
                    continue;
                }
                let (force_owned_load, strict_owned_load) = match *graph.item(position) {
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
                    let following = instruction_positions[index + 1..]
                        .iter()
                        .filter_map(|position| match *graph.item(*position) {
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
                let Item::Instruction(instruction) = graph.item_mut(position) else {
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
        let graph = self
            .graph
            .as_mut()
            .expect("assembler pass requires the block graph");
        let order = graph.order().to_vec();
        for block in order {
            let items = std::mem::take(graph[block].items_mut());
            let mut fused = Vec::with_capacity(items.len());
            let mut index = 0;
            while index < items.len() {
                let Some(Item::Instruction(first)) = items.get(index).copied() else {
                    fused.push(items[index]);
                    index += 1;
                    continue;
                };
                let Some(Item::Instruction(second)) = items.get(index + 1).copied() else {
                    fused.push(items[index]);
                    index += 1;
                    continue;
                };
                if first.has_flag(InstructionFlags::PREVENT_FUSION_WITH_NEXT)
                    || second.has_flag(InstructionFlags::PREVENT_FUSION_WITH_PREVIOUS)
                    || second.has_flag(InstructionFlags::BORROW_UNREACHABLE_ENTRY)
                {
                    fused.push(items[index]);
                    index += 1;
                    continue;
                }
                let (Operand::Value(first_argument), Operand::Value(second_argument)) =
                    (first.operand, second.operand)
                else {
                    fused.push(items[index]);
                    index += 1;
                    continue;
                };
                if first_argument >= 16 || second_argument >= 16 {
                    fused.push(items[index]);
                    index += 1;
                    continue;
                }
                if first.location.line != second.location.line {
                    fused.push(items[index]);
                    index += 1;
                    continue;
                }
                let opcode = match (first.opcode, second.opcode) {
                    (LOAD_FAST_BORROW, LOAD_FAST_BORROW) => LOAD_FAST_BORROW_LOAD_FAST_BORROW,
                    (LOAD_FAST, LOAD_FAST) => LOAD_FAST_LOAD_FAST,
                    (STORE_FAST, LOAD_FAST | LOAD_FAST_BORROW) => STORE_FAST_LOAD_FAST,
                    (STORE_FAST, STORE_FAST) => STORE_FAST_STORE_FAST,
                    _ => {
                        fused.push(items[index]);
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
            *graph[block].items_mut() = fused;
        }
    }
}
