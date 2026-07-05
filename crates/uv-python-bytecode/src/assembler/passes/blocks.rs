use rustc_hash::{FxHashMap, FxHashSet};

use super::super::{
    Assembler, ExceptionRegion, Instruction, InstructionFlags, Item, JUMP_BACKWARD,
    JUMP_BACKWARD_NO_INTERRUPT, JUMP_FORWARD, Label, Operand, SourceLocation,
    block_has_fallthrough, block_jump_target, cleanup_block_creation_order, ends_scope,
};
use crate::target::opcodes::{
    CLEANUP_THROW, END_ASYNC_FOR, END_SEND, LOAD_GLOBAL, NOP, RAISE_VARARGS,
};

impl Assembler {
    pub(in crate::assembler) fn push_cold_blocks_to_end(&mut self) {
        let graph = self
            .graph
            .as_mut()
            .expect("assembler pass requires the block graph");
        if graph.len() < 2 {
            return;
        }

        let order = graph.order().to_vec();
        let mut block_labels = FxHashMap::default();
        for block in order.iter().copied() {
            let label = if let Some(label) = graph[block].first_label() {
                label
            } else {
                let label = Label(self.next_label);
                self.next_label += 1;
                graph.insert_label(block, 0, label);
                label
            };
            block_labels.insert(block, label);
        }

        let positions = order
            .iter()
            .copied()
            .enumerate()
            .map(|(position, block)| (block, position))
            .collect::<FxHashMap<_, _>>();
        let region_memberships = self
            .exception_regions
            .iter()
            .map(|region| {
                let start = graph
                    .label_block(region.start)
                    .and_then(|block| positions.get(&block).copied())
                    .unwrap_or(0);
                let end = graph
                    .label_block(region.end)
                    .and_then(|block| positions.get(&block).copied())
                    .unwrap_or(order.len());
                if start <= end {
                    order[start..end].iter().copied().collect::<FxHashSet<_>>()
                } else {
                    FxHashSet::default()
                }
            })
            .collect::<Vec<_>>();

        let mut warm = FxHashSet::default();
        let mut stack = vec![order[0]];
        while let Some(block) = stack.pop() {
            if !warm.insert(block) {
                continue;
            }
            let position = positions[&block];
            if block_has_fallthrough(graph[block].items())
                && let Some(next) = order.get(position + 1)
            {
                stack.push(*next);
            }
            if let Some(target) = block_jump_target(graph[block].items())
                && let Some(target) = graph.label_block(target)
            {
                stack.push(target);
            }
        }

        let mut cold = FxHashSet::default();
        let mut stack = self
            .exception_regions
            .iter()
            .filter_map(|region| graph.label_block(region.target))
            .filter(|block| !warm.contains(block))
            .collect::<Vec<_>>();
        while let Some(block) = stack.pop() {
            if warm.contains(&block) || !cold.insert(block) {
                continue;
            }
            let position = positions[&block];
            if block_has_fallthrough(graph[block].items())
                && let Some(next) = order.get(position + 1)
            {
                stack.push(*next);
            }
            if let Some(target) = block_jump_target(graph[block].items())
                && let Some(target) = graph.label_block(target)
            {
                stack.push(target);
            }
        }
        if cold.is_empty() {
            return;
        }

        let explicit_jumps = (0..order.len().saturating_sub(1))
            .filter(|position| {
                cold.contains(&order[*position])
                    && block_has_fallthrough(graph[order[*position]].items())
                    && warm.contains(&order[*position + 1])
            })
            .collect::<Vec<_>>();
        for position in explicit_jumps.into_iter().rev() {
            let next = graph.block_at(position + 1);
            let target = if let Some(label) = graph[next].first_label() {
                label
            } else {
                let label = Label(self.next_label);
                self.next_label += 1;
                graph.insert_label(next, 0, label);
                block_labels.insert(next, label);
                label
            };
            let current = graph.block_at(position);
            let (location, exclude_exception_if_extended) = graph[current]
                .last_instruction()
                .map_or((SourceLocation::NONE, false), |instruction| {
                    (
                        instruction.location,
                        instruction.has_flag(InstructionFlags::EXCLUDE_EXCEPTION_IF_EXTENDED),
                    )
                });
            let label = Label(self.next_label);
            self.next_label += 1;
            let block = graph.insert_block(
                position + 1,
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
            block_labels.insert(block, label);
            cold.insert(block);
        }

        let mut order = Vec::with_capacity(graph.len());
        for expected_cold in [false, true] {
            for block in graph.order().iter().copied() {
                if (expected_cold && cold.contains(&block))
                    || (!expected_cold && warm.contains(&block))
                {
                    order.push(block);
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
            .filter_map(|(position, block)| {
                (position + 1 < order.len()
                    && block_jump_target(graph[order[position + 1]].items()).is_some()
                    && graph[*block].items().iter().any(|item| {
                        matches!(item, Item::Instruction(instruction) if instruction.opcode == CLEANUP_THROW)
                    }))
                .then_some(position)
            })
            .collect::<Vec<_>>();
        if cleanup_throw_positions.windows(2).any(|positions| {
            cleanup_block_creation_order(graph[order[positions[0]]].items())
                > cleanup_block_creation_order(graph[order[positions[1]]].items())
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
                    .filter(|(_, block)| cleanup_throw_blocks.contains(block))
                    .collect::<Vec<_>>();
                let Some(inversion) = ordered_cleanup_blocks.windows(2).find(|blocks_in_order| {
                    cleanup_block_creation_order(graph[*blocks_in_order[0].1].items())
                        > cleanup_block_creation_order(graph[*blocks_in_order[1].1].items())
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
                let target = graph[order[position]]
                    .items()
                    .iter()
                    .any(|item| {
                        matches!(item, Item::Instruction(instruction) if instruction.opcode == END_ASYNC_FOR)
                    })
                    .then(|| block_jump_target(graph[order[position + 1]].items()))
                    .flatten()
                    .and_then(|target| graph.label_block(target))?;
                let start = order.iter().position(|block| *block == target)?;
                let mut end = start;
                while end + 1 < order.len()
                    && block_has_fallthrough(graph[order[end]].items())
                {
                    end += 1;
                }
                let exits_scope = graph[order[end]]
                    .last_instruction()
                    .map(|instruction| ends_scope(instruction.opcode));
                (exits_scope == Some(true)).then_some((position + 2, start, end))
            });
            if let Some((insertion_position, start, end)) = terminal_restore {
                let terminal_restore = order.drain(start..=end).collect::<Vec<_>>();
                order.splice(insertion_position..insertion_position, terminal_restore);
            }
        }

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
                let start = block_labels[&order[index]];
                while index < order.len() && members.contains(&order[index]) {
                    index += 1;
                }
                let end = if index < order.len() {
                    block_labels[&order[index]]
                } else if let Some(label) = final_label {
                    label
                } else {
                    let label = Label(self.next_label);
                    self.next_label += 1;
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

        *graph.order_mut() = order;
        graph.rebuild_label_index();
        let item_positions = graph.item_positions();
        let label_positions = item_positions
            .iter()
            .enumerate()
            .filter_map(|(index, position)| match graph.item(*position) {
                Item::Label(label) => Some((*label, index)),
                Item::Instruction(_) => None,
            })
            .collect::<FxHashMap<_, _>>();
        for (index, position) in item_positions.into_iter().enumerate() {
            let Item::Instruction(instruction) = graph.item_mut(position) else {
                continue;
            };
            let target = match instruction.operand {
                Operand::Forward(target) | Operand::Backward(target) => target,
                Operand::Value(_) => continue,
            };
            let Some(target_position) = label_positions.get(&target).copied() else {
                continue;
            };
            if target_position < index {
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
        }
        if let Some(final_label) = final_label {
            graph.push_block(vec![Item::Label(final_label)]);
        }
    }

    pub(in crate::assembler) fn duplicate_exit_blocks(&mut self) {
        let graph = self
            .graph
            .as_mut()
            .expect("assembler pass requires the block graph");
        graph.normalize_blocks(&self.preserved_block_boundaries);
        if graph.len() < 2 {
            return;
        }

        let order = graph.order().to_vec();
        for block in order.iter().copied() {
            if graph[block].first_label().is_none() {
                let label = Label(self.next_label);
                self.next_label += 1;
                graph.insert_label(block, 0, label);
            }
        }
        let positions = order
            .iter()
            .copied()
            .enumerate()
            .map(|(position, block)| (block, position))
            .collect::<FxHashMap<_, _>>();
        let exception_boundary_blocks = self
            .exception_regions
            .iter()
            .flat_map(|region| [region.start, region.end, region.target])
            .filter_map(|label| graph.label_block(label))
            .collect::<FxHashSet<_>>();
        let exception_handler_blocks = self
            .exception_regions
            .iter()
            .filter_map(|region| graph.label_block(region.target))
            .collect::<FxHashSet<_>>();
        let region_memberships = self
            .exception_regions
            .iter()
            .map(|region| {
                let start = graph
                    .label_block(region.start)
                    .and_then(|block| positions.get(&block).copied())
                    .unwrap_or(0);
                let end = graph
                    .label_block(region.end)
                    .and_then(|block| positions.get(&block).copied())
                    .unwrap_or(order.len());
                if start <= end {
                    order[start..end].iter().copied().collect::<FxHashSet<_>>()
                } else {
                    FxHashSet::default()
                }
            })
            .collect::<Vec<_>>();

        let exception_split_labels = self
            .exception_regions
            .iter()
            .flat_map(|region| [region.start, region.end])
            .collect::<FxHashSet<_>>();
        let mut predecessors = order
            .iter()
            .copied()
            .map(|block| (block, 0_usize))
            .collect::<FxHashMap<_, _>>();
        predecessors.insert(order[0], 1);
        for (position, block) in order.iter().copied().enumerate() {
            if block_has_fallthrough(graph[block].items())
                && let Some(next) = order.get(position + 1)
            {
                predecessors.entry(*next).and_modify(|count| *count += 1);
            }
            if let Some(target) = block_jump_target(graph[block].items())
                && let Some(target) = graph.label_block(target)
            {
                predecessors.entry(target).and_modify(|count| *count += 1);
            }
        }

        let exit_without_unique_predecessor = order
            .iter()
            .copied()
            .filter(|block| {
                predecessors[block] > 1
                    && graph[*block].items().iter().all(|item| {
                        !matches!(item, Item::Instruction(instruction) if instruction.location.line >= 0)
                    })
                    && graph[*block]
                        .last_instruction()
                        .is_some_and(|instruction| ends_scope(instruction.opcode))
            })
            .collect::<FxHashSet<_>>();

        let mut inline_copies = FxHashMap::<_, Vec<Vec<Item>>>::default();
        let mut target_copies = FxHashMap::<_, Vec<Vec<Item>>>::default();
        let mut copied_region_exclusions =
            vec![Vec::<(Label, Label)>::new(); self.exception_regions.len()];
        let mut copied_exception_regions = Vec::new();
        let mut drop_blocks = FxHashSet::default();
        let mut remaining_predecessors = predecessors.clone();
        let mut sources = order.clone();
        let mut source_cursor = 0;
        while let Some(&source) = sources.get(source_cursor) {
            source_cursor += 1;
            let Some(target_label) = block_jump_target(graph[source].items()) else {
                continue;
            };
            let Some(target) = graph.label_block(target_label) else {
                continue;
            };
            let source_position = positions[&source];
            let target_position = positions[&target];
            let source_has_fallthrough = block_has_fallthrough(graph[source].items());
            let mut target_chain_end = target_position;
            while target_chain_end + 1 < order.len()
                && block_has_fallthrough(graph[order[target_chain_end]].items())
                && predecessors[&order[target_chain_end + 1]] == 1
                && graph[order[target_chain_end + 1]]
                    .items()
                    .iter()
                    .any(|item| {
                        matches!(item, Item::Label(label) if exception_split_labels.contains(label))
                    })
            {
                target_chain_end += 1;
            }
            let target_blocks = &order[target_position..=target_chain_end];
            let target_pre_fusion_size = target_blocks
                .iter()
                .flat_map(|block| graph[*block].items())
                .map(|item| match item {
                    Item::Instruction(instruction) => {
                        let fused_push_null = instruction.opcode == LOAD_GLOBAL
                            && matches!(instruction.operand, Operand::Value(argument) if argument & 1 != 0);
                        1 + usize::from(fused_push_null)
                    }
                    Item::Label(label) => usize::from(
                        target_chain_end > target_position
                            && exception_split_labels.contains(label),
                    ),
                })
                .sum::<usize>();
            let target_terminal_opcode = target_blocks
                .iter()
                .rev()
                .flat_map(|block| graph[*block].items().iter().rev())
                .find_map(|item| match item {
                    Item::Instruction(instruction) => Some(instruction.opcode),
                    Item::Label(_) => None,
                });
            let target_is_small_exit = target_pre_fusion_size <= 4
                && target_terminal_opcode.is_some_and(|opcode| {
                    ends_scope(opcode)
                        // An exception-boundary split before a raise represents CPython's
                        // `SETUP_FINALLY` inside one basic block. Return paths can also contain
                        // optimized-away `POP_BLOCK` instructions that are not recoverable from
                        // the final region labels, so keep their existing block boundary.
                        && (target_chain_end == target_position || opcode == RAISE_VARARGS)
                });
            let source_allows_small_exit_inlining =
                graph[source].last_instruction().is_some_and(|instruction| {
                    instruction.has_flag(InstructionFlags::INLINE_SMALL_EXIT)
                });
            let target_contains_end_send = target_blocks.iter().any(|block| {
                graph[*block].items().iter().any(|item| {
                    matches!(item, Item::Instruction(instruction) if instruction.opcode == END_SEND)
                })
            });
            let target_has_no_location = target_blocks.iter().all(|block| {
                graph[*block].items().iter().all(|item| {
                    !matches!(item, Item::Instruction(instruction) if instruction.location.line >= 0)
                })
            });
            let target_allows_no_location_block_inlining =
                graph[target].last_instruction().is_some_and(|instruction| {
                    instruction.has_flag(InstructionFlags::ALLOW_NO_LOCATION_BLOCK_INLINING)
                });
            let target_is_no_location_no_fallthrough = target_has_no_location
                && target_allows_no_location_block_inlining
                && !block_has_fallthrough(graph[target].items());
            let inline_small_exit = !source_has_fallthrough
                && target_is_small_exit
                && source_allows_small_exit_inlining
                && !target_contains_end_send
                && !exception_handler_blocks.contains(&target);
            let inline_no_location_block =
                !source_has_fallthrough && target_is_no_location_no_fallthrough;
            let later_jump_predecessor = order[source_position + 1..].iter().any(|predecessor| {
                block_jump_target(graph[*predecessor].items())
                    .and_then(|label| graph.label_block(label))
                    == Some(target)
            });
            if exit_without_unique_predecessor.contains(&target)
                && target_pre_fusion_size > 4
                && remaining_predecessors[&target] > 1
                && later_jump_predecessor
            {
                // CPython's second exit-duplication pass runs after cold blocks move. For a
                // large class footer shared by warm and cold jumps, that makes the cold, later
                // predecessor receive the copy while the warm predecessor keeps the original.
                continue;
            }
            if !(inline_small_exit || inline_no_location_block)
                && (!(exit_without_unique_predecessor.contains(&target)
                    || target_is_no_location_no_fallthrough)
                    || remaining_predecessors[&target] <= 1)
            {
                continue;
            }

            let source_location = graph[source]
                .last_instruction()
                .map_or(SourceLocation::NONE, |instruction| instruction.location);
            let mut copied = target_blocks
                .iter()
                .flat_map(|block| graph[*block].items().iter().copied())
                .collect::<Vec<_>>();
            let preserve_inlined_jump_nop = inline_small_exit
                && source_location.line >= 0
                && graph[source].last_instruction().is_some_and(|instruction| {
                    instruction.has_flag(InstructionFlags::PRESERVE_INLINED_JUMP_NOP)
                        || matches!(
                            (
                                instruction.preserve_direct_inlined_jump_nop,
                                instruction.operand,
                            ),
                            (Some(expected), Operand::Forward(actual)) if expected == actual
                        )
                });
            let copied_label = Label(self.next_label);
            self.next_label += 1;
            let mut replaced_entry_label = false;
            let mut copied_labels = FxHashMap::default();
            for item in &mut copied {
                if let Item::Label(label) = item {
                    let original = *label;
                    if !replaced_entry_label {
                        *label = copied_label;
                        replaced_entry_label = true;
                    } else {
                        *label = Label(self.next_label);
                        self.next_label += 1;
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
                let copied_end = Label(self.next_label);
                self.next_label += 1;
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
            if target_has_no_location && !(inline_no_location_block && predecessors[&source] > 1) {
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
                    let trailing_nop_position = graph[source]
                        .items()
                        .iter()
                        .enumerate()
                        .rev()
                        .filter_map(|(position, item)| match item {
                            Item::Instruction(instruction) => Some((position, instruction.opcode)),
                            Item::Label(_) => None,
                        })
                        .nth(1)
                        .filter(|(_, opcode)| *opcode == NOP)
                        .map(|(position, _)| position);
                    if let Some(position) = trailing_nop_position {
                        exclusion_start = Label(self.next_label);
                        self.next_label += 1;
                        graph.insert_label(source, position, exclusion_start);
                    }
                    let copied_end = Label(self.next_label);
                    self.next_label += 1;
                    copied.push(Item::Label(copied_end));
                    for index in excluded_regions {
                        copied_region_exclusions[index].push((exclusion_start, copied_end));
                    }
                }
            }
            if inline_small_exit || inline_no_location_block {
                if let Some(position) = graph[source]
                    .items()
                    .iter()
                    .rposition(|item| matches!(item, Item::Instruction(_)))
                {
                    graph[source].items_mut().remove(position);
                }
            } else if let Some(Item::Instruction(instruction)) = graph[source]
                .items_mut()
                .iter_mut()
                .rev()
                .find(|item| matches!(item, Item::Instruction(_)))
            {
                instruction.operand = Operand::Forward(copied_label);
            }
            if inline_no_location_block {
                graph.append_to_block(source, copied);
                // The copied jump can turn this source into another eligible unlocated block.
                // Revisit earlier jump predecessors until the chain reaches a fixed point.
                for predecessor in order.iter().copied().take(source_position) {
                    if block_jump_target(graph[predecessor].items())
                        .and_then(|label| graph.label_block(label))
                        == Some(source)
                    {
                        sources.push(predecessor);
                    }
                }
            } else if source_has_fallthrough {
                target_copies.entry(target).or_default().push(copied);
            } else {
                inline_copies.entry(source).or_default().push(copied);
            }
            remaining_predecessors
                .entry(target)
                .and_modify(|count| *count -= 1);
            if remaining_predecessors[&target] == 0 {
                drop_blocks.insert(target);
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

        for target in order.iter().copied() {
            if exception_handler_blocks.contains(&target) {
                continue;
            }
            if remaining_predecessors[&target] == 1
                && let Some(Item::Instruction(first)) = graph[target]
                    .items()
                    .iter()
                    .find(|item| matches!(item, Item::Instruction(_)))
                && first.opcode == NOP
                && first.has_flag(InstructionFlags::CONVERTED_POP_BLOCK)
                && first.location.line < 0
                && !first.has_flag(InstructionFlags::PRESERVE_NO_LOCATION)
                && let Some(location) = order.iter().find_map(|source| {
                    block_jump_target(graph[*source].items())
                        .and_then(|label| graph.label_block(label))
                        .filter(|block| *block == target)
                        .and_then(|_| {
                            graph[*source]
                                .last_instruction()
                                .map(|instruction| instruction.location)
                        })
                })
                && let Some(Item::Instruction(first)) = graph[target]
                    .items_mut()
                    .iter_mut()
                    .find(|item| matches!(item, Item::Instruction(_)))
            {
                // A converted `POP_BLOCK` inherits the line of its sole jump predecessor.
                // This remains observable when another predecessor copied the small exit.
                first.location = location;
                continue;
            }
            if !graph[target].items().iter().all(|item| {
                !matches!(item, Item::Instruction(instruction) if instruction.location.line >= 0)
            }) {
                continue;
            }
            let target_position = positions[&target];
            let predecessor = if target_position > 0
                && block_has_fallthrough(graph[order[target_position - 1]].items())
            {
                Some(order[target_position - 1])
            } else {
                order.iter().copied().find(|source| {
                    block_jump_target(graph[*source].items())
                        .and_then(|label| graph.label_block(label))
                        == Some(target)
                })
            };
            let Some(location) = predecessor.and_then(|source| {
                graph[source]
                    .last_instruction()
                    .map(|instruction| instruction.location)
            }) else {
                continue;
            };
            if let Some(Item::Instruction(instruction)) = graph[target]
                .items_mut()
                .iter_mut()
                .find(|item| matches!(item, Item::Instruction(_)))
                && !instruction.has_flag(InstructionFlags::PRESERVE_NO_LOCATION)
            {
                instruction.location = location;
            }
        }

        let mut final_order = Vec::new();
        for block in order {
            if !drop_blocks.contains(&block) {
                final_order.push(block);
            } else if exception_boundary_blocks.contains(&block) {
                graph[block]
                    .items_mut()
                    .retain(|item| matches!(item, Item::Label(_)));
                final_order.push(block);
            }
            for copied in inline_copies.remove(&block).unwrap_or_default() {
                final_order.extend(
                    graph.allocate_blocks_from_items(copied, &self.preserved_block_boundaries),
                );
            }
            for copied in target_copies
                .remove(&block)
                .unwrap_or_default()
                .into_iter()
                .rev()
            {
                final_order.extend(
                    graph.allocate_blocks_from_items(copied, &self.preserved_block_boundaries),
                );
            }
        }
        *graph.order_mut() = final_order;
        graph.rebuild_label_index();
        // `inline_no_location_block` temporarily appends its copy to the source so earlier
        // predecessors can see the copied terminal jump. Restore explicit block boundaries now.
        graph.normalize_blocks(&self.preserved_block_boundaries);
    }
}
