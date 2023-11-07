use std::fmt;
use std::hash::{Hash, Hasher};
use std::ops::Deref;

#[derive(Clone)]
pub enum SmallVec<T> {
    Empty,
    One([T; 1]),
    Two([T; 2]),
    Flexible(Vec<T>),
}

impl<T> SmallVec<T> {
    pub fn empty() -> Self {
        Self::Empty
    }

    pub fn one(t: T) -> Self {
        Self::One([t])
    }

    pub fn as_slice(&self) -> &[T] {
        match self {
            Self::Empty => &[],
            Self::One(v) => v,
            Self::Two(v) => v,
            Self::Flexible(v) => v,
        }
    }

    pub fn push(&mut self, new: T) {
        *self = match std::mem::take(self) {
            Self::Empty => Self::One([new]),
            Self::One([v1]) => Self::Two([v1, new]),
            Self::Two([v1, v2]) => Self::Flexible(vec![v1, v2, new]),
            Self::Flexible(mut v) => {
                v.push(new);
                Self::Flexible(v)
            }
        }
    }

    pub fn pop(&mut self) -> Option<T> {
        match std::mem::take(self) {
            Self::Empty => None,
            Self::One([v1]) => {
                *self = Self::Empty;
                Some(v1)
            }
            Self::Two([v1, v2]) => {
                *self = Self::One([v1]);
                Some(v2)
            }
            Self::Flexible(mut v) => {
                let out = v.pop();
                *self = Self::Flexible(v);
                out
            }
        }
    }

    pub fn clear(&mut self) {
        if let Self::Flexible(mut v) = std::mem::take(self) {
            v.clear();
            *self = Self::Flexible(v);
        } // else: self already eq Empty from the take
    }

    pub fn iter(&self) -> std::slice::Iter<'_, T> {
        self.as_slice().iter()
    }
}

impl<T> Default for SmallVec<T> {
    fn default() -> Self {
        Self::Empty
    }
}

impl<T> Deref for SmallVec<T> {
    type Target = [T];

    fn deref(&self) -> &Self::Target {
        self.as_slice()
    }
}

impl<'a, T> IntoIterator for &'a SmallVec<T> {
    type Item = &'a T;

    type IntoIter = std::slice::Iter<'a, T>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

impl<T: Eq> Eq for SmallVec<T> {}

impl<T: PartialEq> PartialEq for SmallVec<T> {
    fn eq(&self, other: &Self) -> bool {
        self.as_slice() == other.as_slice()
    }
}

impl<T: fmt::Debug> fmt::Debug for SmallVec<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.as_slice().fmt(f)
    }
}

impl<T: Hash> Hash for SmallVec<T> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.len().hash(state);
        Hash::hash_slice(self.as_slice(), state);
    }
}

#[cfg(feature = "serde")]
impl<T: serde::Serialize> serde::Serialize for SmallVec<T> {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        serde::Serialize::serialize(self.as_slice(), s)
    }
}

#[cfg(feature = "serde")]
impl<'de, T: serde::Deserialize<'de>> serde::Deserialize<'de> for SmallVec<T> {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        struct SmallVecVisitor<T> {
            marker: std::marker::PhantomData<T>,
        }

        impl<'de, T> serde::de::Visitor<'de> for SmallVecVisitor<T>
        where
            T: serde::Deserialize<'de>,
        {
            type Value = SmallVec<T>;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str("a sequence")
            }

            fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
            where
                A: serde::de::SeqAccess<'de>,
            {
                let mut values = SmallVec::empty();
                while let Some(value) = seq.next_element()? {
                    values.push(value);
                }
                Ok(values)
            }
        }

        let visitor = SmallVecVisitor {
            marker: Default::default(),
        };
        d.deserialize_seq(visitor)
    }
}

impl<T> IntoIterator for SmallVec<T> {
    type Item = T;
    type IntoIter = SmallVecIntoIter<T>;

    fn into_iter(self) -> Self::IntoIter {
        match self {
            SmallVec::Empty => SmallVecIntoIter::Empty,
            SmallVec::One(a) => SmallVecIntoIter::One(a.into_iter()),
            SmallVec::Two(a) => SmallVecIntoIter::Two(a.into_iter()),
            SmallVec::Flexible(v) => SmallVecIntoIter::Flexible(v.into_iter()),
        }
    }
}

pub enum SmallVecIntoIter<T> {
    Empty,
    One(<[T; 1] as IntoIterator>::IntoIter),
    Two(<[T; 2] as IntoIterator>::IntoIter),
    Flexible(<Vec<T> as IntoIterator>::IntoIter),
}

impl<T> Iterator for SmallVecIntoIter<T> {
    type Item = T;

    fn next(&mut self) -> Option<Self::Item> {
        match self {
            SmallVecIntoIter::Empty => None,
            SmallVecIntoIter::One(it) => it.next(),
            SmallVecIntoIter::Two(it) => it.next(),
            SmallVecIntoIter::Flexible(it) => it.next(),
        }
    }
}

// TESTS #######################################################################

#[cfg(test)]
pub mod tests {
    use super::*;
    use proptest::prelude::*;

    proptest! {
        #[test]
        fn push_and_pop(commands: Vec<Option<u8>>) {
            let mut v = vec![];
            let mut sv = SmallVec::Empty;
            for command in commands {
                match command {
                    Some(i) => {
                        v.push(i);
                        sv.push(i);
                    }
                    None => {
                        assert_eq!(v.pop(), sv.pop());
                    }
                }
                assert_eq!(v.as_slice(), sv.as_slice());
            }
        }
    }
}
