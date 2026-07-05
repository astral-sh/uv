use rustc_hash::FxHashMap;

use super::{AssembledCode, Assembler, AssemblerStage, Item, Label, Operand, extended_arg_count};
use crate::CompileError;

mod control_flow;
mod locals;
mod swaps;

impl Assembler {
    #[cfg(any(test, debug_assertions))]
    pub(in crate::assembler) fn validate_structure(
        &self,
        stage: AssemblerStage,
    ) -> Result<(), CompileError> {
        let invariant_error = |message: String| {
            CompileError::Internal(format!(
                "assembler invariant failed after {}: {message}",
                stage.name()
            ))
        };

        let mut label_positions = FxHashMap::default();
        for (index, item) in self.items.iter().enumerate() {
            let Item::Label(label) = item else {
                continue;
            };
            if label_positions.insert(*label, index).is_some() {
                return Err(invariant_error(format!(
                    "label {} is bound more than once",
                    label.0
                )));
            }
        }

        for (index, item) in self.items.iter().enumerate() {
            let Item::Instruction(instruction) = item else {
                continue;
            };
            let (target, is_forward) = match instruction.operand {
                Operand::Value(_) => continue,
                Operand::Forward(target) => (target, true),
                Operand::Backward(target) => (target, false),
            };
            if !instruction.opcode.has_jump() {
                return Err(invariant_error(format!(
                    "instruction {index} uses a label operand with non-jump opcode {}",
                    instruction.opcode.code()
                )));
            }
            let Some(target_index) = label_positions.get(&target).copied() else {
                return Err(invariant_error(format!(
                    "instruction {index} targets unbound label {}",
                    target.0
                )));
            };
            if is_forward && target_index <= index {
                return Err(invariant_error(format!(
                    "instruction {index} has a forward operand to label {} at {target_index}",
                    target.0
                )));
            }
            if !is_forward && target_index >= index {
                return Err(invariant_error(format!(
                    "instruction {index} has a backward operand to label {} at {target_index}",
                    target.0
                )));
            }
        }

        for (index, region) in self.exception_regions.iter().enumerate() {
            let position = |label: Label, role: &str| {
                label_positions.get(&label).copied().ok_or_else(|| {
                    invariant_error(format!(
                        "exception region {index} has an unbound {role} label {}",
                        label.0
                    ))
                })
            };
            let start = position(region.start, "start")?;
            let end = position(region.end, "end")?;
            position(region.target, "target")?;
            if stage.requires_ordered_exception_regions() && start > end {
                return Err(invariant_error(format!(
                    "exception region {index} starts at {start} after it ends at {end}"
                )));
            }
        }

        if stage.requires_full_reachability() {
            let reachable = self.reachable_items();
            if let Some(index) = self.items.iter().enumerate().find_map(|(index, item)| {
                matches!(item, Item::Instruction(_))
                    .then_some(index)
                    .filter(|index| !reachable[*index])
            }) {
                return Err(invariant_error(format!(
                    "instruction {index} remains unreachable"
                )));
            }
        }

        Ok(())
    }

    #[cfg(test)]
    pub(crate) fn finish(self) -> Result<Vec<u8>, CompileError> {
        self.finish_code(1, 0, 0)
            .map(|assembled| assembled.bytecode)
    }

    pub(crate) fn finish_code(
        mut self,
        first_line_number: u32,
        local_count: usize,
        parameter_count: usize,
    ) -> Result<AssembledCode, CompileError> {
        let mut removed_max_depth = self.remove_unreachable_instructions();
        #[cfg(any(test, debug_assertions))]
        self.validate_structure(AssemblerStage::RemoveUnreachableInitial)?;
        self.optimize_boolean_conversions();
        #[cfg(any(test, debug_assertions))]
        self.validate_structure(AssemblerStage::OptimizeBooleanConversions)?;
        self.thread_forward_jumps();
        #[cfg(any(test, debug_assertions))]
        self.validate_structure(AssemblerStage::ThreadForwardJumps)?;
        if let Some(depth) = self.remove_unreachable_instructions() {
            removed_max_depth = Some(removed_max_depth.map_or(depth, |current| current.max(depth)));
        }
        #[cfg(any(test, debug_assertions))]
        self.validate_structure(AssemblerStage::RemoveUnreachableAfterThreading)?;
        self.remove_redundant_forward_jumps();
        #[cfg(any(test, debug_assertions))]
        self.validate_structure(AssemblerStage::RemoveRedundantForwardJumpsEarly)?;
        self.optimize_redundant_store_fast();
        #[cfg(any(test, debug_assertions))]
        self.validate_structure(AssemblerStage::OptimizeRedundantStoreFast)?;
        self.optimize_swap_runs();
        #[cfg(any(test, debug_assertions))]
        self.validate_structure(AssemblerStage::OptimizeSwapRuns)?;
        self.apply_static_swaps();
        #[cfg(any(test, debug_assertions))]
        self.validate_structure(AssemblerStage::ApplyStaticSwaps)?;
        self.duplicate_exit_blocks();
        #[cfg(any(test, debug_assertions))]
        self.validate_structure(AssemblerStage::DuplicateExitBlocks)?;
        self.add_checks_for_uninitialized_loads(local_count, parameter_count);
        #[cfg(any(test, debug_assertions))]
        self.validate_structure(AssemblerStage::AddChecksForUninitializedLoads)?;
        self.fuse_superinstructions();
        #[cfg(any(test, debug_assertions))]
        self.validate_structure(AssemblerStage::FuseSuperinstructions)?;
        self.push_cold_blocks_to_end();
        #[cfg(any(test, debug_assertions))]
        self.validate_structure(AssemblerStage::PushColdBlocksToEnd)?;
        self.remove_redundant_checked_loads();
        #[cfg(any(test, debug_assertions))]
        self.validate_structure(AssemblerStage::RemoveRedundantCheckedLoads)?;
        self.propagate_locations_within_blocks();
        #[cfg(any(test, debug_assertions))]
        self.validate_structure(AssemblerStage::PropagateLocationsWithinBlocks)?;
        self.remove_redundant_swaps_before_pops();
        #[cfg(any(test, debug_assertions))]
        self.validate_structure(AssemblerStage::RemoveRedundantSwapsBeforePops)?;
        self.remove_redundant_nops();
        #[cfg(any(test, debug_assertions))]
        self.validate_structure(AssemblerStage::RemoveRedundantNops)?;
        self.remove_redundant_forward_jumps();
        #[cfg(any(test, debug_assertions))]
        self.validate_structure(AssemblerStage::RemoveRedundantForwardJumpsLate)?;
        self.optimize_load_fast();
        #[cfg(any(test, debug_assertions))]
        self.validate_structure(AssemblerStage::OptimizeLoadFast)?;
        let instruction_count = self.instruction_count();
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
                let jump_base = opcode_position + 1 + u32::from(instruction.opcode.caches());
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
                #[cfg(any(test, debug_assertions))]
                self.validate_structure(AssemblerStage::FinalLayout)?;
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
                return Ok(AssembledCode {
                    bytecode: self.encode(&extended_args, &resolved_arguments),
                    line_table,
                    exception_table,
                    max_depth,
                    removed_max_depth,
                });
            }
        }

        Err(CompileError::Internal(
            "jump layout did not converge".to_string(),
        ))
    }
}
