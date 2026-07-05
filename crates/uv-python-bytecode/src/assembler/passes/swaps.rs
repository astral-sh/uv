use super::super::*;

impl Assembler {
    pub(in crate::assembler) fn remove_redundant_swaps_before_pops(&mut self) {
        let mut index = 0;
        while index < self.items.len() {
            let Item::Instruction(instruction) = self.items[index] else {
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
            let preceded_by_swap = self.items[..index].iter().rev().find_map(|item| {
                if let Item::Instruction(instruction) = item {
                    Some(instruction.opcode == SWAP)
                } else {
                    None
                }
            }) == Some(true);
            let following = self.items[index + 1..]
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
    pub(in crate::assembler) fn optimize_swap_runs(&mut self) {
        const VISITED: usize = usize::MAX;

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
                        || ends_scope(instruction.opcode) =>
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
                while run_end < instruction_indices.len() {
                    let Item::Instruction(instruction) = self.items[instruction_indices[run_end]]
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
                for instruction_index in &instruction_indices[position..run_end] {
                    let Item::Instruction(instruction) = self.items[*instruction_index] else {
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
                                &mut self.items[instruction_indices[current]]
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
                        &mut self.items[instruction_indices[current]]
                    else {
                        unreachable!();
                    };
                    instruction.opcode = NOP;
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
                if instruction.opcode == NOP {
                    index += 1;
                    continue;
                }
                return swappable(&instruction).then_some(index);
            }
            None
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
                        || ends_scope(instruction.opcode) =>
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
                if swap.opcode != SWAP {
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
                swap.opcode = NOP;
                swap.operand = Operand::Value(0);
                self.items.swap(first, last);
            }
            block_start = block_end;
        }
    }
}
