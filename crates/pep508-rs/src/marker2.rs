#![allow(unused, dead_code)]

use std::{
    borrow::Cow,
    collections::{hash_map::Entry, HashMap, HashSet},
};

use smallvec::{smallvec, SmallVec};

use crate::{MarkerExpression, MarkerValueString, MarkerValueVersion};

#[repr(u8)]
#[derive(PartialOrd, Ord, PartialEq, Eq, Hash, Clone, Copy)]
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
pub(crate) struct Manager {
    nodes: Vec<Node>,
    node_ids: HashMap<Node, NodeId>,

    functions: Vec<Function>,
    function_ids: HashMap<Variable, HashMap<MarkerExpression, FunctionId>>,

    cache: HashMap<(NodeId, NodeId), NodeId>,
}

pub(crate) struct Function {
    var: Variable,
    expr: MarkerExpression,
}

impl Manager {
    fn create_func(&mut self, func: Function) -> FunctionId {
        let variables = self.function_ids.entry(func.var).or_default();

        match variables.entry(func.expr.clone()) {
            Entry::Occupied(id) => *id.get(),
            Entry::Vacant(entry) => {
                let id = FunctionId(self.functions.len());
                self.functions.push(func);
                entry.insert(id);
                id
            }
        }
    }

    fn create_node(&mut self, var: Variable, children: Children) -> NodeId {
        let node = Node { var, children };

        match self.node_ids.entry(node.clone()) {
            Entry::Occupied(id) => *id.get(),
            Entry::Vacant(entry) => {
                let id = NodeId(self.nodes.len());
                self.nodes.push(node);
                entry.insert(id);
                id
            }
        }
    }

    fn node(&self, id: NodeId) -> Cow<'_, Node> {
        let node = &self.nodes[id.index()];
        if id.is_complement() {
            Cow::Owned(node.flip())
        } else {
            Cow::Borrowed(node)
        }
    }

    fn func(&self, id: FunctionId) -> &Function {
        &self.functions[id.index()]
    }

    pub(crate) fn terminal(&mut self, func: Function) -> NodeId {
        let var = func.var;
        let func = self.create_func(func);
        let child = self.create_node(var, smallvec![]);
        self.create_node(var, smallvec![(func, child)])
    }

    pub(crate) fn is_disjoint(&mut self, x: NodeId, y: NodeId) -> bool {
        self.and(x, y) == NodeId::FALSE
    }

    pub(crate) fn is_satisfiable(&self, x: NodeId) -> bool {
        x != NodeId::FALSE
    }

    pub(crate) fn not(&mut self, x: NodeId) -> NodeId {
        x.not()
    }

    pub(crate) fn or(&mut self, x: NodeId, y: NodeId) -> NodeId {
        self.and(x.not(), y.not()).not()
    }

    pub(crate) fn and(&mut self, x: NodeId, y: NodeId) -> NodeId {
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

        if let Some(result) = self.cache.get(&(x, y)) {
            return *result;
        }

        // Perform Shannon Expansion for each possible value of the higher order variable.
        let (x_var, y_var) = (self.node(x).var, self.node(y).var);

        let (var, children) = if x_var < y_var {
            let functions = self.function_ids[&x_var].clone();
            let children = functions
                .into_values()
                .filter_map(|func| {
                    let child = self.node(x).find_child(func)?;
                    Some((func, self.and(child, y)))
                })
                .collect();

            (x_var, children)
        } else if y_var < x_var {
            let functions = self.function_ids[&y_var].clone();
            let children = functions
                .into_values()
                .filter_map(|func| {
                    let child = self.node(y).find_child(func)?;
                    Some((func, self.and(child, x)))
                })
                .collect();

            (y_var, children)
        } else {
            let functions = self.function_ids[&x_var].clone();
            let children = functions
                .into_values()
                .filter_map(|func| {
                    let x_child = self.node(x).find_child(func)?;
                    let y_child = self.node(y).find_child(func)?;
                    Some((func, self.and(x_child, y_child)))
                })
                .collect();

            (x_var, children)
        };

        let node = self.create_node(var, children);
        self.cache.insert((x, y), node);

        node
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct NodeId(usize);

impl NodeId {
    const FALSE: NodeId = NodeId::new(0, false);
    const TRUE: NodeId = NodeId::new(0, true);

    const fn new(index: usize, negated: bool) -> NodeId {
        NodeId((index << 1) | (negated as usize))
    }

    fn index(self) -> usize {
        self.0 >> 1
    }

    fn is_complement(self) -> bool {
        (self.0 & 1) == 1
    }

    fn not(self) -> NodeId {
        NodeId(self.0 ^ 1)
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct FunctionId(usize);

impl FunctionId {
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
    fn find_child(&self, func: FunctionId) -> Option<NodeId> {
        self.children
            .iter()
            .find(|(f, _)| *f == func)
            .map(|((_, node))| node)
            .copied()
    }

    fn flip(&self) -> Node {
        Node {
            var: self.var,
            children: self.children.iter().map(|(f, n)| (*f, n.not())).collect(),
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::MarkerExpression;

    use super::{Function, Manager, Variable};

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
        let mut m = Manager::default();
        let x = m.terminal(f("extra == 'foo'"));
        assert!(m.is_satisfiable(x));

        let x = m.terminal(f("extra != 'foo'"));
        assert!(!m.is_satisfiable(x));
    }
}
