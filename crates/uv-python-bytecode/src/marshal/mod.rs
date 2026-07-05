mod graph;
mod key;
mod writer;

#[cfg(test)]
mod tests;

pub(crate) use writer::encode_code;

const FLAG_REF: u8 = 0x80;
const TYPE_NONE: u8 = b'N';
const TYPE_FALSE: u8 = b'F';
const TYPE_TRUE: u8 = b'T';
const TYPE_ELLIPSIS: u8 = b'.';
const TYPE_INT: u8 = b'i';
const TYPE_LONG: u8 = b'l';
const TYPE_BINARY_FLOAT: u8 = b'g';
const TYPE_BINARY_COMPLEX: u8 = b'y';
const TYPE_BYTES: u8 = b's';
const TYPE_SMALL_TUPLE: u8 = b')';
const TYPE_TUPLE: u8 = b'(';
const TYPE_CODE: u8 = b'c';
const TYPE_UNICODE: u8 = b'u';
const TYPE_INTERNED: u8 = b't';
const TYPE_ASCII: u8 = b'a';
const TYPE_ASCII_INTERNED: u8 = b'A';
const TYPE_SHORT_ASCII: u8 = b'z';
const TYPE_SHORT_ASCII_INTERNED: u8 = b'Z';
const TYPE_REF: u8 = b'r';
const TYPE_SLICE: u8 = b':';
const TYPE_FROZENSET: u8 = b'>';
