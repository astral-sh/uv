#![allow(unused, dead_code)]

use std::{
    borrow::Cow,
    collections::{hash_map::Entry, HashMap, HashSet},
    fmt, iter,
    sync::Mutex,
};

use once_cell::sync::Lazy;
use smallvec::{smallvec, SmallVec};

use crate::{MarkerExpression, MarkerValueString, MarkerValueVersion};

#[repr(u8)]
#[derive(PartialOrd, Ord, PartialEq, Eq, Hash, Clone, Copy, Debug)]
enum Variable {
    // `MarkerValueVersion`
    ImplementationVersion,
    PythonFullVersion,
    PythonVersion,
    // `MarkerValueString`
    ImplementationName,
    OsName,
    OsNameDeprecated,
    PlatformMachine,
    PlatformMachineDeprecated,
    PlatformPythonImplementation,
    PlatformPythonImplementationDeprecated,
    PythonImplementationDeprecated,
    PlatformRelease,
    PlatformSystem,
    PlatformVersion,
    PlatformVersionDeprecated,
    SysPlatform,
    SysPlatformDeprecated,
    // `extra`
    Extra,
}

impl From<MarkerValueVersion> for Variable {
    fn from(value: MarkerValueVersion) -> Self {
        match value {
            MarkerValueVersion::ImplementationVersion => Variable::ImplementationVersion,
            MarkerValueVersion::PythonFullVersion => Variable::PythonFullVersion,
            MarkerValueVersion::PythonVersion => Variable::PythonVersion,
        }
    }
}

impl From<MarkerValueString> for Variable {
    fn from(value: MarkerValueString) -> Self {
        match value {
            MarkerValueString::ImplementationName => Variable::ImplementationName,
            MarkerValueString::OsName => Variable::OsName,
            MarkerValueString::OsNameDeprecated => Variable::OsNameDeprecated,
            MarkerValueString::PlatformMachine => Variable::PlatformMachine,
            MarkerValueString::PlatformMachineDeprecated => Variable::PlatformMachineDeprecated,
            MarkerValueString::PlatformPythonImplementation => {
                Variable::PlatformPythonImplementation
            }
            MarkerValueString::PlatformPythonImplementationDeprecated => {
                Variable::PlatformPythonImplementationDeprecated
            }
            MarkerValueString::PythonImplementationDeprecated => {
                Variable::PythonImplementationDeprecated
            }
            MarkerValueString::PlatformRelease => Variable::PlatformRelease,
            MarkerValueString::PlatformSystem => Variable::PlatformSystem,
            MarkerValueString::PlatformVersion => Variable::PlatformVersion,
            MarkerValueString::PlatformVersionDeprecated => Variable::PlatformVersionDeprecated,
            MarkerValueString::SysPlatform => Variable::SysPlatform,
            MarkerValueString::SysPlatformDeprecated => Variable::SysPlatformDeprecated,
        }
    }
}

#[derive(Default)]
pub(crate) struct Interner {
    shared: InternerShared,
    state: Mutex<InternerState>,
}

impl Interner {
    pub(crate) fn terminal(&self, func: Function) -> NodeId {
        let mut state = self.state.lock().unwrap();
        state.terminal(func, &self.shared)
    }

    pub(crate) fn is_disjoint(&self, x: NodeId, y: NodeId) -> bool {
        self.and(x, y) == NodeId::FALSE
    }

    pub(crate) fn is_satisfiable(&self, x: NodeId) -> bool {
        x != NodeId::FALSE
    }

    pub(crate) fn not(&self, x: NodeId) -> NodeId {
        x.flip()
    }

    pub(crate) fn or(&self, x: NodeId, y: NodeId) -> NodeId {
        self.and(x.flip(), y.flip()).flip()
    }

    pub(crate) fn and(&self, x: NodeId, y: NodeId) -> NodeId {
        let mut state = self.state.lock().unwrap();
        state.and(x, y, &self.shared)
    }
}

#[derive(Default)]
struct InternerShared {
    nodes: boxcar::Vec<Node>,
    functions: boxcar::Vec<Function>,
}

#[derive(Default)]
struct InternerState {
    node_ids: HashMap<Node, NodeId>,
    function_ids: HashMap<Variable, HashMap<MarkerExpression, FunctionId>>,
    cache: HashMap<(NodeId, NodeId), NodeId>,
}

static INTERNER: Lazy<Interner> = Lazy::new(Interner::default);

#[derive(Debug)]
pub(crate) struct Function {
    var: Variable,
    expr: MarkerExpression,
}

impl InternerState {
    fn create_func(&mut self, func: Function, shared: &InternerShared) -> FunctionId {
        let variables = self.function_ids.entry(func.var).or_default();

        match variables.entry(func.expr.clone()) {
            Entry::Occupied(id) => *id.get(),
            Entry::Vacant(entry) => {
                let id = FunctionId::new(shared.functions.push(func));
                entry.insert(id);
                id
            }
        }
    }

    fn create_node(
        &mut self,
        var: Variable,
        mut children: Children,
        shared: &InternerShared,
    ) -> NodeId {
        // // Keep `FunctionId::ELSE` at the end.
        children.sort();

        let mut node = Node { var, children };
        let (_, first) = node.children[0];

        // Canonical Form: First child is never complemented.
        let mut flipped = false;
        if first.is_complement() {
            node = node.flip_children();
            flipped = true;
        }

        if node.children.iter().all(|(_, x)| *x == first) {
            if flipped {
                return first.flip();
            } else {
                return first;
            }
        }

        match self.node_ids.entry(node.clone()) {
            Entry::Occupied(id) => *id.get(),
            Entry::Vacant(entry) => {
                let id = NodeId::new(shared.nodes.push(node), false);
                entry.insert(id);

                if flipped {
                    return id.flip();
                } else {
                    return id;
                }
            }
        }
    }

    pub(crate) fn terminal(&mut self, func: Function, shared: &InternerShared) -> NodeId {
        let var = func.var;
        let hi = self.create_func(func, shared);
        let children = smallvec![(FunctionId::ELSE, NodeId::FALSE), (hi, NodeId::TRUE)];
        self.create_node(var, children, shared)
    }

    pub(crate) fn and(&mut self, x: NodeId, y: NodeId, shared: &InternerShared) -> NodeId {
        if x == NodeId::TRUE {
            return y;
        }

        if y == NodeId::TRUE {
            return x;
        }

        if x == y {
            return x;
        }

        if x == NodeId::FALSE || y == NodeId::FALSE {
            return NodeId::FALSE;
        }

        // One is complemented but not the other.
        if x.index() == y.index() {
            return NodeId::FALSE;
        }

        if let Some(result) = self.cache.get(&(x, y)) {
            return *result;
        }

        // Perform Shannon Expansion for each possible value of the higher order variable.
        let (x_node, y_node) = (shared.node(x), shared.node(y));
        let (var, children) = if x_node.var < y_node.var {
            let functions = self.function_ids[&x_node.var].clone();
            let children = functions
                .into_values()
                .chain(iter::once(FunctionId::ELSE))
                .filter_map(|func| {
                    let child = x_node.find_child(x, func)?;
                    Some((func, self.and(child, y, shared)))
                })
                .collect::<Children>();

            (x_node.var, children)
        } else if y_node.var < x_node.var {
            let functions = self.function_ids[&y_node.var].clone();
            let children = functions
                .into_values()
                .chain(iter::once(FunctionId::ELSE))
                .filter_map(|func| {
                    let child = y_node.find_child(y, func)?;
                    Some((func, self.and(child, x, shared)))
                })
                .collect();

            (y_node.var, children)
        } else {
            let functions = self.function_ids[&x_node.var].clone();
            let children = functions
                .into_values()
                .chain(iter::once(FunctionId::ELSE))
                .filter_map(|func| {
                    let x_child = x_node.find_child(x, func)?;
                    let y_child = y_node.find_child(y, func)?;
                    Some((func, self.and(x_child, y_child, shared)))
                })
                .collect();

            (x_node.var, children)
        };

        let node = self.create_node(var, children, shared);
        self.cache.insert((x, y), node);

        node
    }
}

impl InternerShared {
    /// Returns the node without accounting for the negation.
    fn node(&self, id: NodeId) -> &Node {
        &self.nodes[id.index()]
    }

    fn func(&self, id: FunctionId) -> &Function {
        &self.functions[id.index()]
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub(crate) struct NodeId(usize);

impl NodeId {
    const FALSE: NodeId = NodeId(0);
    const TRUE: NodeId = NodeId(1);

    const fn new(index: usize, negated: bool) -> NodeId {
        NodeId(((index + 1) << 1) | (negated as usize))
    }

    fn index(self) -> usize {
        (self.0 >> 1) - 1
    }

    fn is_complement(self) -> bool {
        (self.0 & 1) == 1
    }

    fn flip(self) -> NodeId {
        NodeId(self.0 ^ 1)
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub(crate) struct FunctionId(usize);

impl fmt::Debug for FunctionId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if *self == FunctionId::ELSE {
            write!(f, "else")
        } else {
            write!(f, "{:?}", INTERNER.shared.func(*self))
        }
    }
}

impl FunctionId {
    const ELSE: FunctionId = FunctionId(usize::MAX);

    fn new(index: usize) -> FunctionId {
        FunctionId(index)
    }

    fn index(self) -> usize {
        self.0
    }
}

#[derive(PartialEq, Eq, Hash, Clone)]
pub(crate) struct Node {
    var: Variable,
    children: Children,
}

type Children = SmallVec<[(FunctionId, NodeId); 3]>;

impl Node {
    fn find_child(&self, id: NodeId, func: FunctionId) -> Option<NodeId> {
        self.children
            .iter()
            .find(|(f, _)| *f == func)
            .map(|((_, node))| node)
            // Restore the canonical negation form.
            .map(|&node| {
                if id.is_complement() {
                    node.flip()
                } else {
                    node
                }
            })
    }

    fn flip_children(&self) -> Node {
        Node {
            var: self.var,
            children: self.children.iter().map(|(f, n)| (*f, n.flip())).collect(),
        }
    }
}

impl fmt::Debug for NodeId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if *self == NodeId::FALSE {
            return write!(f, "false");
        }

        if *self == NodeId::TRUE {
            return write!(f, "true");
        }

        let node = INTERNER.shared.node(*self);
        for (func, child) in &node.children {
            // Restore the canonical negation form.
            let child = if self.is_complement() {
                child.flip()
            } else {
                *child
            };

            if *func == FunctionId::ELSE {
                write!(f, "else {{\n{:?}\n}}", child)?;
            } else {
                let func = INTERNER.shared.func(*func);
                write!(f, "if ({}) {{\n{:?}\n}}\n", func.expr, child)?;
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use crate::{
        marker2::{NodeId, INTERNER},
        MarkerExpression,
    };

    use super::{Function, Interner, Variable};

    fn f(s: &str) -> Function {
        let expr = s.parse::<MarkerExpression>().unwrap();
        let var = match &expr {
            MarkerExpression::Version { key, .. } => key.clone().into(),
            MarkerExpression::VersionInverted { key, .. } => key.clone().into(),
            MarkerExpression::String { key, .. } => key.clone().into(),
            MarkerExpression::StringInverted { key, .. } => key.clone().into(),
            MarkerExpression::Extra { operator, name } => Variable::Extra,
            _ => panic!(),
        };

        Function { var, expr }
    }

    #[test]
    fn test() {
        let m = &*INTERNER;

        let a = m.terminal(f("extra == 'foo'"));
        assert!(m.is_satisfiable(a));

        let b = m.terminal(f("os_name == 'foo'"));
        let c = m.or(a, b);
        assert!(m.is_satisfiable(c));

        let d = m.and(a, m.not(a));
        assert!(!m.is_satisfiable(d));

        let e = m.or(d, b);
        assert!(m.is_satisfiable(e));

        let f = m.or(c, m.not(c));
        assert!(m.is_satisfiable(f));
        assert!(f == NodeId::TRUE);
    }
}
