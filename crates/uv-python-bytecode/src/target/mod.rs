//! Metadata for the exact CPython bytecode target.

mod cpython_3_14_5;

pub(crate) use cpython_3_14_5::code_flags;
pub(crate) use cpython_3_14_5::local_kinds;
pub(crate) use cpython_3_14_5::opcodes;
pub(crate) use cpython_3_14_5::operands;
pub(crate) use cpython_3_14_5::{
    TARGET_COMMIT, TARGET_IMPLEMENTATION, TARGET_MAGIC_NUMBER, TARGET_TAG, TARGET_VERSION,
    is_conditional_jump, is_scope_exit, is_unconditional_jump, num_popped, num_pushed,
};

/// An opcode and its inline-cache width for the selected CPython target.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(crate) struct Opcode {
    code: u8,
    caches: u8,
}

impl Opcode {
    /// This constructor is private so that generated target metadata is the sole numeric authority.
    const fn new(code: u8, caches: u8) -> Self {
        Self { code, caches }
    }

    pub(crate) const fn code(self) -> u8 {
        self.code
    }

    pub(crate) const fn caches(self) -> u8 {
        self.caches
    }
}
