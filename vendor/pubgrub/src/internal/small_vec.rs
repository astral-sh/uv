use std::fmt;
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

#[cfg(feature = "serde")]
impl<T: serde::Serialize> serde::Serialize for SmallVec<T> {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        serde::Serialize::serialize(self.as_slice(), s)
    }
}

#[cfg(feature = "serde")]
impl<'de, T: serde::Deserialize<'de>> serde::Deserialize<'de> for SmallVec<T> {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let items: Vec<T> = serde::Deserialize::deserialize(d)?;

        let mut v = Self::empty();
        for item in items {
            v.push(item);
        }
        Ok(v)
    }
}

// TESTS #######################################################################

#[cfg(test)]
pub mod tests {
    use super::*;
    use proptest::prelude::*;

    proptest! {
        #[test]
        fn push_and_pop(comands: Vec<Option<u8>>) {
            let mut v = vec![];
            let mut sv = SmallVec::Empty;
            for comand in comands {
                match comand {
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
