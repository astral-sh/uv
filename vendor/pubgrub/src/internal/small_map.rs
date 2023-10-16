use crate::type_aliases::Map;
use std::hash::Hash;

#[derive(Debug, Clone)]
pub(crate) enum SmallMap<K, V> {
    Empty,
    One([(K, V); 1]),
    Two([(K, V); 2]),
    Flexible(Map<K, V>),
}

impl<K: PartialEq + Eq + Hash, V> SmallMap<K, V> {
    pub(crate) fn get(&self, key: &K) -> Option<&V> {
        match self {
            Self::Empty => None,
            Self::One([(k, v)]) if k == key => Some(v),
            Self::One(_) => None,
            Self::Two([(k1, v1), _]) if key == k1 => Some(v1),
            Self::Two([_, (k2, v2)]) if key == k2 => Some(v2),
            Self::Two(_) => None,
            Self::Flexible(data) => data.get(key),
        }
    }

    pub(crate) fn get_mut(&mut self, key: &K) -> Option<&mut V> {
        match self {
            Self::Empty => None,
            Self::One([(k, v)]) if k == key => Some(v),
            Self::One(_) => None,
            Self::Two([(k1, v1), _]) if key == k1 => Some(v1),
            Self::Two([_, (k2, v2)]) if key == k2 => Some(v2),
            Self::Two(_) => None,
            Self::Flexible(data) => data.get_mut(key),
        }
    }

    pub(crate) fn remove(&mut self, key: &K) -> Option<V> {
        let out;
        *self = match std::mem::take(self) {
            Self::Empty => {
                out = None;
                Self::Empty
            }
            Self::One([(k, v)]) => {
                if key == &k {
                    out = Some(v);
                    Self::Empty
                } else {
                    out = None;
                    Self::One([(k, v)])
                }
            }
            Self::Two([(k1, v1), (k2, v2)]) => {
                if key == &k1 {
                    out = Some(v1);
                    Self::One([(k2, v2)])
                } else if key == &k2 {
                    out = Some(v2);
                    Self::One([(k1, v1)])
                } else {
                    out = None;
                    Self::Two([(k1, v1), (k2, v2)])
                }
            }
            Self::Flexible(mut data) => {
                out = data.remove(key);
                Self::Flexible(data)
            }
        };
        out
    }

    pub(crate) fn insert(&mut self, key: K, value: V) {
        *self = match std::mem::take(self) {
            Self::Empty => Self::One([(key, value)]),
            Self::One([(k, v)]) => {
                if key == k {
                    Self::One([(k, value)])
                } else {
                    Self::Two([(k, v), (key, value)])
                }
            }
            Self::Two([(k1, v1), (k2, v2)]) => {
                if key == k1 {
                    Self::Two([(k1, value), (k2, v2)])
                } else if key == k2 {
                    Self::Two([(k1, v1), (k2, value)])
                } else {
                    let mut data: Map<K, V> = Map::with_capacity_and_hasher(3, Default::default());
                    data.insert(key, value);
                    data.insert(k1, v1);
                    data.insert(k2, v2);
                    Self::Flexible(data)
                }
            }
            Self::Flexible(mut data) => {
                data.insert(key, value);
                Self::Flexible(data)
            }
        };
    }
}

impl<K: Clone + PartialEq + Eq + Hash, V: Clone> SmallMap<K, V> {
    /// Merge two hash maps.
    ///
    /// When a key is common to both,
    /// apply the provided function to both values.
    /// If the result is None, remove that key from the merged map,
    /// otherwise add the content of the Some(_).
    pub(crate) fn merge<'a>(
        &'a mut self,
        map_2: impl Iterator<Item = (&'a K, &'a V)>,
        f: impl Fn(&V, &V) -> Option<V>,
    ) {
        for (key, val_2) in map_2 {
            match self.get_mut(key) {
                None => {
                    self.insert(key.clone(), val_2.clone());
                }
                Some(val_1) => match f(val_1, val_2) {
                    None => {
                        self.remove(key);
                    }
                    Some(merged_value) => *val_1 = merged_value,
                },
            }
        }
    }
}

impl<K, V> Default for SmallMap<K, V> {
    fn default() -> Self {
        Self::Empty
    }
}

impl<K, V> SmallMap<K, V> {
    pub(crate) fn len(&self) -> usize {
        match self {
            Self::Empty => 0,
            Self::One(_) => 1,
            Self::Two(_) => 2,
            Self::Flexible(data) => data.len(),
        }
    }
}

impl<K: Eq + Hash + Clone, V: Clone> SmallMap<K, V> {
    pub(crate) fn as_map(&self) -> Map<K, V> {
        match self {
            Self::Empty => Map::default(),
            Self::One([(k, v)]) => {
                let mut map = Map::with_capacity_and_hasher(1, Default::default());
                map.insert(k.clone(), v.clone());
                map
            }
            Self::Two(data) => {
                let mut map = Map::with_capacity_and_hasher(2, Default::default());
                for (k, v) in data {
                    map.insert(k.clone(), v.clone());
                }
                map
            }
            Self::Flexible(data) => data.clone(),
        }
    }
}

enum IterSmallMap<'a, K, V> {
    Inline(std::slice::Iter<'a, (K, V)>),
    Map(std::collections::hash_map::Iter<'a, K, V>),
}

impl<'a, K: 'a, V: 'a> Iterator for IterSmallMap<'a, K, V> {
    type Item = (&'a K, &'a V);

    fn next(&mut self) -> Option<Self::Item> {
        match self {
            IterSmallMap::Inline(inner) => inner.next().map(|(k, v)| (k, v)),
            IterSmallMap::Map(inner) => inner.next(),
        }
    }
}

impl<K, V> SmallMap<K, V> {
    pub(crate) fn iter(&self) -> impl Iterator<Item = (&K, &V)> {
        match self {
            Self::Empty => IterSmallMap::Inline([].iter()),
            Self::One(data) => IterSmallMap::Inline(data.iter()),
            Self::Two(data) => IterSmallMap::Inline(data.iter()),
            Self::Flexible(data) => IterSmallMap::Map(data.iter()),
        }
    }
}
