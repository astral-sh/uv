use super::{
    Assembler, AssemblerStage, Instruction, InstructionFlags, Item, Operand, SourceLocation,
};
use crate::target::opcodes::*;

#[test]
fn instruction_constructors_set_audited_defaults() {
    let regular = Instruction::new(LOAD_CONST, Operand::Value(0), SourceLocation::NONE, None);
    assert_eq!(regular.flags, InstructionFlags::DEFAULT);
    assert!(regular.has_flag(InstructionFlags::INLINE_SMALL_EXIT));
    assert!(regular.has_flag(InstructionFlags::ALLOW_JUMP_THREADING_TARGET));

    let synthetic = Instruction::synthetic(NOP, Operand::Value(0), SourceLocation::NONE, None);
    assert!(!synthetic.has_flag(InstructionFlags::INLINE_SMALL_EXIT));
    assert!(synthetic.has_flag(InstructionFlags::ALLOW_JUMP_THREADING_TARGET));
    assert_eq!(
        synthetic.preserve_direct_inlined_jump_nop, None,
        "synthetic instructions do not inherit jump provenance"
    );
    assert_eq!(synthetic.normalized_exception_owner, None);
}

#[test]
fn fusion_uses_each_flags_propagation_policy() {
    let mut first = Instruction::new(
        LOAD_FAST,
        Operand::Value(1),
        SourceLocation::new(2, 2, 0, 1),
        Some(1),
    );
    for flag in [
        InstructionFlags::FORCE_OWNED_LOAD,
        InstructionFlags::ALLOW_NO_LOCATION_BLOCK_INLINING,
        InstructionFlags::DEFER_REDUNDANT_JUMP_REMOVAL,
        InstructionFlags::CONVERTED_POP_BLOCK,
        InstructionFlags::EXCLUDE_EXCEPTION_IF_EXTENDED,
        InstructionFlags::PREVENT_FUSION_WITH_NEXT,
        InstructionFlags::PRESERVE_INLINED_JUMP_NOP,
    ] {
        first.insert_flag(flag);
    }
    first.remove_flag(InstructionFlags::ALLOW_JUMP_THREADING_TARGET);
    first.preserve_direct_inlined_jump_nop = Some(super::Label(1));
    first.normalized_exception_owner = Some(false);

    let mut second = Instruction::new(
        LOAD_FAST,
        Operand::Value(2),
        SourceLocation::new(2, 2, 2, 3),
        Some(2),
    );
    for flag in [
        InstructionFlags::STRICT_OWNED_LOAD,
        InstructionFlags::PRESERVE_NO_LOCATION,
        InstructionFlags::PRESERVE_NOP_AFTER_JUMP_THREADING,
        InstructionFlags::EXCLUDE_EXCEPTION,
        InstructionFlags::PREVENT_FUSION_WITH_PREVIOUS,
        InstructionFlags::BORROW_UNREACHABLE_ENTRY,
        InstructionFlags::PRESERVE_INLINED_JUMP_NOP,
    ] {
        second.insert_flag(flag);
    }
    second.remove_flag(InstructionFlags::INLINE_SMALL_EXIT);
    second.preserve_direct_inlined_jump_nop = Some(super::Label(2));
    second.normalized_exception_owner = Some(true);

    let fused = Instruction::fused(LOAD_FAST_LOAD_FAST, Operand::Value(0x12), first, second);

    for flag in [
        InstructionFlags::FORCE_OWNED_LOAD,
        InstructionFlags::STRICT_OWNED_LOAD,
        InstructionFlags::ALLOW_NO_LOCATION_BLOCK_INLINING,
        InstructionFlags::PRESERVE_NO_LOCATION,
        InstructionFlags::DEFER_REDUNDANT_JUMP_REMOVAL,
        InstructionFlags::PRESERVE_NOP_AFTER_JUMP_THREADING,
        InstructionFlags::CONVERTED_POP_BLOCK,
        InstructionFlags::EXCLUDE_EXCEPTION,
        InstructionFlags::EXCLUDE_EXCEPTION_IF_EXTENDED,
    ] {
        assert!(fused.has_flag(flag), "missing merged flag {flag:?}");
    }
    for flag in [
        InstructionFlags::INLINE_SMALL_EXIT,
        InstructionFlags::PRESERVE_INLINED_JUMP_NOP,
        InstructionFlags::ALLOW_JUMP_THREADING_TARGET,
        InstructionFlags::PREVENT_FUSION_WITH_NEXT,
        InstructionFlags::PREVENT_FUSION_WITH_PREVIOUS,
        InstructionFlags::BORROW_UNREACHABLE_ENTRY,
    ] {
        assert!(!fused.has_flag(flag), "unexpected merged flag {flag:?}");
    }
    assert_eq!(fused.location, first.location);
    assert_eq!(fused.depth_after, second.depth_after);
    assert_eq!(fused.preserve_direct_inlined_jump_nop, None);
    assert_eq!(fused.normalized_exception_owner, Some(false));

    let mut first_edge = Instruction::new(LOAD_FAST, Operand::Value(1), SourceLocation::NONE, None);
    first_edge.insert_flag(InstructionFlags::PREVENT_FUSION_WITH_PREVIOUS);
    first_edge.insert_flag(InstructionFlags::BORROW_UNREACHABLE_ENTRY);
    let mut second_edge =
        Instruction::new(LOAD_FAST, Operand::Value(2), SourceLocation::NONE, None);
    second_edge.insert_flag(InstructionFlags::PREVENT_FUSION_WITH_NEXT);
    let fused_edges = Instruction::fused(
        LOAD_FAST_LOAD_FAST,
        Operand::Value(0x12),
        first_edge,
        second_edge,
    );
    for flag in [
        InstructionFlags::INLINE_SMALL_EXIT,
        InstructionFlags::ALLOW_JUMP_THREADING_TARGET,
        InstructionFlags::PREVENT_FUSION_WITH_NEXT,
        InstructionFlags::PREVENT_FUSION_WITH_PREVIOUS,
        InstructionFlags::BORROW_UNREACHABLE_ENTRY,
    ] {
        assert!(
            fused_edges.has_flag(flag),
            "missing edge-selected flag {flag:?}"
        );
    }
}

#[test]
fn owned_fast_loads_use_real_opcodes_with_provenance() {
    let mut assembler = Assembler::default();
    assembler.emit_owned_fast_with_depth(3, 1);
    let Some(Item::Instruction(instruction)) = assembler.items.last() else {
        panic!("owned load was not emitted");
    };
    assert_eq!(instruction.opcode, LOAD_FAST);
    assert!(instruction.has_flag(InstructionFlags::FORCE_OWNED_LOAD));
    assert!(!instruction.has_flag(InstructionFlags::STRICT_OWNED_LOAD));
}

#[test]
fn structural_validation_reports_the_pass_and_broken_label() {
    let mut assembler = Assembler::default();
    let target = assembler.label();
    assembler.emit_operand(JUMP_FORWARD, Operand::Forward(target));

    let error = assembler
        .validate_structure(AssemblerStage::ThreadForwardJumps)
        .unwrap_err();
    let message = error.to_string();
    assert!(message.contains("thread_forward_jumps"));
    assert!(message.contains("unbound label"));
}

#[test]
fn structural_validation_rejects_duplicate_labels() {
    let mut assembler = Assembler::default();
    let label = assembler.label();
    assembler.mark(label);
    assembler.mark(label);

    let error = assembler
        .validate_structure(AssemblerStage::FinalLayout)
        .unwrap_err();
    assert!(error.to_string().contains("bound more than once"));
}

#[test]
fn structural_validation_rejects_a_misdirected_operand() {
    let mut assembler = Assembler::default();
    let target = assembler.label();
    assembler.mark(target);
    assembler.emit_operand(JUMP_FORWARD, Operand::Forward(target));

    let error = assembler
        .validate_structure(AssemblerStage::OptimizeBooleanConversions)
        .unwrap_err();
    assert!(error.to_string().contains("forward operand"));
}

#[test]
fn structural_validation_rejects_a_label_operand_on_a_non_jump() {
    let mut assembler = Assembler::default();
    let target = assembler.label();
    assembler.emit_operand(NOP, Operand::Forward(target));
    assembler.mark(target);

    let error = assembler
        .validate_structure(AssemblerStage::RemoveUnreachableInitial)
        .unwrap_err();
    assert!(
        error
            .to_string()
            .contains("label operand with non-jump opcode")
    );
}

#[test]
fn exception_regions_only_require_final_order_after_cfg_passes() {
    let mut assembler = Assembler::default();
    let end = assembler.label();
    let start = assembler.label();
    let target = assembler.label();
    assembler.mark(end);
    assembler.mark(start);
    assembler.mark(target);
    assembler.add_exception_region(start, end, target, 0, false);

    assembler
        .validate_structure(AssemblerStage::RemoveUnreachableInitial)
        .unwrap();
    let error = assembler
        .validate_structure(AssemblerStage::FinalLayout)
        .unwrap_err();
    assert!(error.to_string().contains("starts at"));
}

#[test]
fn removes_unreachable_jumps() {
    let jump = JUMP_FORWARD;
    let resume = RESUME;
    let mut assembler = Assembler::default();
    let start = assembler.label();
    let end = assembler.label();

    assembler.mark(start);
    assembler.emit(resume, 0);
    assembler.emit_operand(jump, Operand::Forward(end));
    assembler.emit_backward(jump, start);
    assembler.mark(end);

    assert_eq!(assembler.finish().unwrap(), [RESUME.code(), 0]);
}

#[test]
fn emits_extended_arguments() {
    let mut assembler = Assembler::default();
    assembler.emit(LOAD_CONST, 0x1234);
    assert_eq!(
        assembler.finish().unwrap(),
        [EXTENDED_ARG.code(), 0x12, LOAD_CONST.code(), 0x34]
    );
}

#[test]
fn removes_an_overwritten_local_store() {
    let mut assembler = Assembler::default();
    assembler.emit(STORE_FAST, 1);
    assembler.emit(STORE_FAST, 1);
    assert_eq!(
        assembler.finish().unwrap(),
        [POP_TOP.code(), 0, STORE_FAST.code(), 1]
    );
}
