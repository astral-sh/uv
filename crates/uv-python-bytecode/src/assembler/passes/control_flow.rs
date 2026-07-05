use rustc_hash::{FxHashMap, FxHashSet};

use super::super::{
    COMPARE_OP, CONTAINS_OP, COPY, IS_OP, InstructionFlags, Item, JUMP_BACKWARD,
    JUMP_BACKWARD_NO_INTERRUPT, JUMP_FORWARD, NOP, NOT_TAKEN, Operand, POP_JUMP_IF_FALSE,
    POP_JUMP_IF_TRUE, POP_TOP, STORE_FAST, TO_BOOL, UNARY_NOT, ends_scope, is_unconditional_jump,
};
use crate::assembler::Assembler;
use crate::target::Opcode;
use crate::target::operands::ComparisonOperation;

impl Assembler {
    /// Replaces an overwritten local store with a stack pop.
    ///
    /// CPython applies this peephole before superinstruction fusion when two
    /// adjacent stores target the same local on the same traced line.
    pub(in crate::assembler) fn optimize_redundant_store_fast(&mut self) {
        let graph = self
            .graph
            .as_mut()
            .expect("assembler pass requires the block graph");
        let order = graph.order().to_vec();
        for block in order {
            let items = graph[block].items_mut();
            for index in 0..items.len().saturating_sub(1) {
                let [Item::Instruction(first), Item::Instruction(second)] =
                    &mut items[index..index + 2]
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
    }

    pub(in crate::assembler) fn remove_unreachable_instructions(&mut self) -> Option<u32> {
        let graph = self
            .graph
            .as_mut()
            .expect("assembler pass requires the block graph");
        let reachable = graph.reachable_blocks(&self.exception_regions);
        let mut removed_max_depth = None;
        let order = graph.order().to_vec();
        let mut empty = FxHashSet::default();
        for block in order {
            if reachable.contains(&block) {
                continue;
            }
            graph[block].items_mut().retain(|item| match item {
                Item::Label(_) => true,
                Item::Instruction(instruction) => {
                    if let Some(depth) = instruction.depth_after {
                        removed_max_depth =
                            Some(removed_max_depth.map_or(depth, |max: u32| max.max(depth)));
                    }
                    false
                }
            });
            if graph[block].items().is_empty() {
                empty.insert(block);
            }
        }
        if !empty.is_empty() {
            graph.order_mut().retain(|block| !empty.contains(block));
        }
        removed_max_depth
    }

    pub(in crate::assembler) fn reachable_items(&self) -> Vec<bool> {
        if let Some(graph) = &self.graph {
            graph.item_reachability(&self.exception_regions)
        } else {
            super::super::block_graph::BlockGraph::from_items(
                self.items.clone(),
                &self.preserved_block_boundaries,
            )
            .item_reachability(&self.exception_regions)
        }
    }

    pub(in crate::assembler) fn optimize_boolean_conversions(&mut self) {
        let graph = self
            .graph
            .as_mut()
            .expect("assembler pass requires the block graph");
        let order = graph.order().to_vec();
        for block in order {
            let items = graph[block].items_mut();
            for index in 0..items.len().saturating_sub(1) {
                let [Item::Instruction(current), Item::Instruction(next)] =
                    &mut items[index..index + 2]
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
        let graph = self
            .graph
            .as_ref()
            .expect("assembler pass requires the block graph");
        let mut positions = graph.item_positions();
        let mut items = positions
            .iter()
            .map(|position| *graph.item(*position))
            .collect::<Vec<_>>();
        let mut conditional_targets = FxHashMap::default();
        for (index, item) in items.iter().enumerate() {
            let Item::Label(label) = item else {
                continue;
            };
            let following = items[index + 1..]
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

        let mut fallthrough_indices = items
            .iter()
            .enumerate()
            .filter_map(|(index, item)| {
                let Item::Instruction(instruction) = item else {
                    return None;
                };
                if !is_value_preserving_jump(&items, index) {
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
        {
            let graph = self
                .graph
                .as_mut()
                .expect("assembler pass requires the block graph");
            for index in fallthrough_indices.into_iter().rev() {
                graph.insert_item(positions[index], Item::Label(fallthrough_labels[&index]));
            }
        }

        let graph = self
            .graph
            .as_ref()
            .expect("assembler pass requires the block graph");
        positions = graph.item_positions();
        items = positions
            .iter()
            .map(|position| *graph.item(*position))
            .collect();
        let value_preserving_jumps = (0..items.len())
            .filter(|index| is_value_preserving_jump(&items, *index))
            .collect::<FxHashSet<_>>();
        let graph = self
            .graph
            .as_mut()
            .expect("assembler pass requires the block graph");
        for (index, position) in positions.iter().copied().enumerate() {
            let Item::Instruction(instruction) = graph.item_mut(position) else {
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

        let graph = self
            .graph
            .as_ref()
            .expect("assembler pass requires the block graph");
        items = graph.iter_items().copied().collect();
        let mut jump_targets = FxHashMap::default();
        for (index, item) in items.iter().enumerate() {
            let Item::Label(label) = item else {
                continue;
            };
            let Some(Item::Instruction(instruction)) = items[index + 1..]
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

        let graph = self
            .graph
            .as_mut()
            .expect("assembler pass requires the block graph");
        let order = graph.order().to_vec();
        for block in order {
            for item in graph[block].items_mut() {
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
                    let Some((next_operand, next_opcode)) = jump_targets.get(&target).copied()
                    else {
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
    }

    pub(in crate::assembler) fn remove_redundant_forward_jumps(&mut self) {
        let mut index = 0;
        loop {
            let graph = self
                .graph
                .as_ref()
                .expect("assembler pass requires the block graph");
            let positions = graph.item_positions();
            if index >= positions.len() {
                break;
            }
            let items = positions
                .iter()
                .map(|position| *graph.item(*position))
                .collect::<Vec<_>>();
            let Item::Instruction(instruction) = items[index] else {
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
                let Item::Instruction(instruction) = self
                    .graph
                    .as_mut()
                    .expect("assembler pass requires the block graph")
                    .item_mut(positions[index])
                else {
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
            let target_is_next = items[index + 1..]
                .iter()
                .take_while(|item| matches!(item, Item::Label(_)))
                .any(|item| matches!(item, Item::Label(label) if *label == target));
            if !target_is_next {
                index += 1;
                continue;
            }
            let previous_line = items[..index].iter().rev().find_map(|item| {
                if let Item::Instruction(instruction) = item {
                    Some(instruction.location.line)
                } else {
                    None
                }
            });
            let next_line = items[index + 1..].iter().find_map(|item| {
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
                self.graph
                    .as_mut()
                    .expect("assembler pass requires the block graph")
                    .remove_item(positions[index]);
                if instruction.has_flag(InstructionFlags::EXCLUDE_EXCEPTION_IF_EXTENDED)
                    && let Some(previous) =
                        positions[..index].iter().rev().copied().find(|position| {
                            matches!(
                                self.graph
                                    .as_ref()
                                    .expect("assembler pass requires the block graph")
                                    .item(*position),
                                Item::Instruction(_)
                            )
                        })
                {
                    let Item::Instruction(previous) = self
                        .graph
                        .as_mut()
                        .expect("assembler pass requires the block graph")
                        .item_mut(previous)
                    else {
                        unreachable!();
                    };
                    previous.insert_flag(InstructionFlags::EXCLUDE_EXCEPTION_IF_EXTENDED);
                }
                if instruction.location.line >= 0
                    && let Some(next) = self
                        .graph
                        .as_ref()
                        .expect("assembler pass requires the block graph")
                        .item_positions()
                        .into_iter()
                        .skip(index)
                        .find(|position| {
                            matches!(
                                self.graph
                                    .as_ref()
                                    .expect("assembler pass requires the block graph")
                                    .item(*position),
                                Item::Instruction(_)
                            )
                        })
                    && let Item::Instruction(next) = self
                        .graph
                        .as_mut()
                        .expect("assembler pass requires the block graph")
                        .item_mut(next)
                    && next.location.line < 0
                    && !next.has_flag(InstructionFlags::PRESERVE_NO_LOCATION)
                    && !next.has_flag(InstructionFlags::ALLOW_NO_LOCATION_BLOCK_INLINING)
                {
                    next.location = instruction.location;
                }
            } else {
                let Item::Instruction(instruction) = self
                    .graph
                    .as_mut()
                    .expect("assembler pass requires the block graph")
                    .item_mut(positions[index])
                else {
                    unreachable!();
                };
                instruction.opcode = NOP;
                instruction.operand = Operand::Value(0);
                index += 1;
            }
        }
    }

    pub(in crate::assembler) fn propagate_locations_within_blocks(&mut self) {
        let graph = self
            .graph
            .as_mut()
            .expect("assembler pass requires the block graph");
        let order = graph.order().to_vec();
        for block in order {
            let mut previous = None;
            let mut block_has_instruction = false;
            for item in graph[block].items_mut() {
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
    }

    pub(in crate::assembler) fn remove_redundant_nops(&mut self) {
        let mut index = 0;
        loop {
            let graph = self
                .graph
                .as_ref()
                .expect("assembler pass requires the block graph");
            let positions = graph.item_positions();
            if index >= positions.len() {
                break;
            }
            let items = positions
                .iter()
                .map(|position| *graph.item(*position))
                .collect::<Vec<_>>();
            let Item::Instruction(instruction) = items[index] else {
                index += 1;
                continue;
            };
            if instruction.opcode != NOP {
                index += 1;
                continue;
            }
            if instruction.location.line < 0 {
                self.graph
                    .as_mut()
                    .expect("assembler pass requires the block graph")
                    .remove_item(positions[index]);
                continue;
            }
            if instruction.has_flag(InstructionFlags::CONVERTED_POP_BLOCK)
                && let Some(previous) = items[..index].iter().rev().find_map(|item| {
                    if let Item::Instruction(instruction) = item {
                        Some(instruction)
                    } else {
                        None
                    }
                })
                && previous.location.line == instruction.location.line
                && !(ends_scope(previous.opcode) || is_unconditional_jump(previous.opcode))
            {
                self.graph
                    .as_mut()
                    .expect("assembler pass requires the block graph")
                    .remove_item(positions[index]);
                continue;
            }
            let next_location = items[index + 1..].iter().find_map(|item| {
                if let Item::Instruction(instruction) = item {
                    Some(instruction.location)
                } else {
                    None
                }
            });
            if next_location.is_some_and(|location| location.line == instruction.location.line) {
                self.graph
                    .as_mut()
                    .expect("assembler pass requires the block graph")
                    .remove_item(positions[index]);
            } else {
                index += 1;
            }
        }
    }
}
