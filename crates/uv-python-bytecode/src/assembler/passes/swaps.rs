use rustc_hash::FxHashSet;

use super::super::{Instruction, Item, NOP, Operand, POP_TOP, STORE_FAST, SWAP, ends_scope};
use crate::assembler::Assembler;
use crate::assembler::block_graph::{BlockGraph, ItemPosition};

impl Assembler {
    pub(in crate::assembler) fn remove_redundant_swaps_before_pops(&mut self) {
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
            if instruction.opcode != SWAP {
                index += 1;
                continue;
            }
            let Operand::Value(depth) = instruction.operand else {
                index += 1;
                continue;
            };
            let pop_count = usize::try_from(depth).unwrap_or(usize::MAX);
            let preceded_by_swap = items[..index].iter().rev().find_map(|item| {
                if let Item::Instruction(instruction) = item {
                    Some(instruction.opcode == SWAP)
                } else {
                    None
                }
            }) == Some(true);
            let following = items[index + 1..]
                .iter()
                .filter_map(|item| {
                    if let Item::Instruction(instruction) = item {
                        Some(instruction.opcode)
                    } else {
                        None
                    }
                })
                .take(pop_count)
                .collect::<Vec<_>>();
            if preceded_by_swap
                && following.len() == pop_count
                && following.iter().all(|opcode| *opcode == POP_TOP)
            {
                self.graph
                    .as_mut()
                    .expect("assembler pass requires the block graph")
                    .remove_item(positions[index]);
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
    pub(in crate::assembler) fn optimize_swap_runs(&mut self) {
        const VISITED: usize = usize::MAX;

        let graph = self
            .graph
            .as_ref()
            .expect("assembler pass requires the block graph");
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
        for region in &self.exception_regions {
            block_labels.extend([region.start, region.end, region.target]);
        }
        block_labels.extend(self.preserved_block_boundaries.iter().copied());
        let blocks = graph.partition_positions(&block_labels, |instruction| {
            !matches!(instruction.operand, Operand::Value(_)) || ends_scope(instruction.opcode)
        });
        let graph = self
            .graph
            .as_mut()
            .expect("assembler pass requires the block graph");
        for block in blocks {
            let instruction_positions = block
                .into_iter()
                .filter(|position| matches!(graph.item(*position), Item::Instruction(_)))
                .collect::<Vec<_>>();
            let mut position = 0;
            while position < instruction_positions.len() {
                let Item::Instruction(first) = *graph.item(instruction_positions[position]) else {
                    unreachable!();
                };
                if first.opcode != SWAP {
                    position += 1;
                    continue;
                }
                let Operand::Value(first_depth) = first.operand else {
                    unreachable!();
                };
                let mut depth = usize::try_from(first_depth).unwrap();
                let mut run_end = position + 1;
                let mut multiple_swaps = false;
                while run_end < instruction_positions.len() {
                    let Item::Instruction(instruction) =
                        *graph.item(instruction_positions[run_end])
                    else {
                        unreachable!();
                    };
                    if instruction.opcode == SWAP {
                        let Operand::Value(argument) = instruction.operand else {
                            unreachable!();
                        };
                        depth = depth.max(usize::try_from(argument).unwrap());
                        multiple_swaps = true;
                    } else if instruction.opcode != NOP {
                        break;
                    }
                    run_end += 1;
                }
                if !multiple_swaps {
                    position += 1;
                    continue;
                }

                let mut stack = (0..depth).collect::<Vec<_>>();
                for instruction_position in &instruction_positions[position..run_end] {
                    let Item::Instruction(instruction) = *graph.item(*instruction_position) else {
                        unreachable!();
                    };
                    if instruction.opcode == SWAP {
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
                                graph.item_mut(instruction_positions[current])
                            else {
                                unreachable!();
                            };
                            instruction.opcode = SWAP;
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
                        graph.item_mut(instruction_positions[current])
                    else {
                        unreachable!();
                    };
                    instruction.opcode = NOP;
                    instruction.operand = Operand::Value(0);
                }
                position = run_end;
            }
        }
    }

    /// Applies stack swaps statically by reordering local stores and pops.
    ///
    /// This is the `apply_static_swaps` half of CPython's `swaptimize` pass and
    /// must run before adjacent local stores are fused. Reverse traversal applies
    /// consecutive swaps from right to left, matching CPython's per-run handoff.
    pub(in crate::assembler) fn apply_static_swaps(&mut self) {
        fn swappable(instruction: &Instruction) -> bool {
            matches!(instruction.opcode, POP_TOP | STORE_FAST)
        }

        fn stored_local(instruction: &Instruction) -> Option<u32> {
            (instruction.opcode == STORE_FAST).then(|| match instruction.operand {
                Operand::Value(argument) => argument,
                Operand::Forward(_) | Operand::Backward(_) => unreachable!(),
            })
        }

        fn next_swappable(
            graph: &BlockGraph,
            positions: &[ItemPosition],
            mut index: usize,
            end: usize,
            line: Option<i32>,
        ) -> Option<usize> {
            while index < end {
                let Item::Instruction(instruction) = *graph.item(positions[index]) else {
                    index += 1;
                    continue;
                };
                if line.is_some_and(|line| instruction.location.line != line) {
                    return None;
                }
                if instruction.opcode == NOP {
                    index += 1;
                    continue;
                }
                return swappable(&instruction).then_some(index);
            }
            None
        }

        let graph = self
            .graph
            .as_ref()
            .expect("assembler pass requires the block graph");
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
        for region in &self.exception_regions {
            block_labels.extend([region.start, region.end, region.target]);
        }
        block_labels.extend(self.preserved_block_boundaries.iter().copied());
        let blocks = graph.partition_positions(&block_labels, |instruction| {
            !matches!(instruction.operand, Operand::Value(_)) || ends_scope(instruction.opcode)
        });
        let graph = self
            .graph
            .as_mut()
            .expect("assembler pass requires the block graph");
        for positions in blocks {
            let mut index = positions.len();
            while index > 0 {
                index -= 1;
                let Item::Instruction(swap) = *graph.item(positions[index]) else {
                    continue;
                };
                if swap.opcode != SWAP {
                    continue;
                }
                let Operand::Value(depth) = swap.operand else {
                    unreachable!();
                };
                let Some(first) =
                    next_swappable(graph, &positions, index + 1, positions.len(), None)
                else {
                    continue;
                };
                let Item::Instruction(first_instruction) = *graph.item(positions[first]) else {
                    unreachable!();
                };
                let line = (first_instruction.location.line >= 0)
                    .then_some(first_instruction.location.line);
                let mut last = first;
                let mut valid = true;
                for _ in 1..depth {
                    let Some(next) =
                        next_swappable(graph, &positions, last + 1, positions.len(), line)
                    else {
                        valid = false;
                        break;
                    };
                    last = next;
                }
                if !valid {
                    continue;
                }
                let Item::Instruction(first_instruction) = *graph.item(positions[first]) else {
                    unreachable!();
                };
                let Item::Instruction(last_instruction) = *graph.item(positions[last]) else {
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
                        || positions[first + 1..last].iter().any(|position| {
                            let Item::Instruction(instruction) = graph.item(*position) else {
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

                let Item::Instruction(swap) = graph.item_mut(positions[index]) else {
                    unreachable!();
                };
                swap.opcode = NOP;
                swap.operand = Operand::Value(0);
                let first_item = *graph.item(positions[first]);
                let last_item = *graph.item(positions[last]);
                *graph.item_mut(positions[first]) = last_item;
                *graph.item_mut(positions[last]) = first_item;
            }
        }
    }
}
