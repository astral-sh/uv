use std::fmt::Debug;

use uv_macros::DebugNoInline;

macro_rules! define_error {
    ($derive:path) => {
        #[derive($derive)]
        pub enum Error<'a, T, const N: usize>
        where
            T: Copy,
        {
            Unit,
            Tuple(T, &'a str),
            Named { value: T, r#type: [u8; N] },
            Recursive(Box<Self>),
            EmptyTuple(),
            EmptyStruct {},
            r#Raw { __formatter: T, __field_0: T },
        }

        pub(super) fn examples() -> [Error<'static, u8, 2>; 7] {
            [
                Error::Unit,
                Error::Tuple(1, "download"),
                Error::Named {
                    value: 2,
                    r#type: [3, 4],
                },
                Error::Recursive(Box::new(Error::Tuple(5, "retry"))),
                Error::EmptyTuple(),
                Error::EmptyStruct {},
                Error::Raw {
                    __formatter: 6,
                    __field_0: 7,
                },
            ]
        }
    };
}

pub mod builtin {
    define_error!(Debug);
}

pub mod no_inline {
    define_error!(uv_macros::DebugNoInline);
}

#[test]
fn matches_builtin_debug() {
    for (expected, actual) in builtin::examples().iter().zip(no_inline::examples().iter()) {
        assert_eq!(format!("{actual:?}"), format!("{expected:?}"));
        assert_eq!(format!("{actual:#?}"), format!("{expected:#?}"));
    }
}

#[derive(DebugNoInline)]
enum Empty {}

fn require_debug<T: Debug>() {}

#[test]
fn supports_empty_enums() {
    require_debug::<Empty>();
}
