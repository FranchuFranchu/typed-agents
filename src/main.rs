#![feature(let_chains)]

pub mod run;
pub mod syntax;

use std::{collections::BTreeMap, rc::Rc};

use itertools::iproduct;
use run::{AgentId, InteractionSystem, Net, Tree, VarId};
use slotmap::{DefaultKey, SlotMap};
use syntax::Statement;

use crate::{run::InteractionRule, syntax::CodeParser};

#[derive(Clone, Debug)]
pub struct UntypedMatch {
    id: AgentId,
    aux: Vec<Tree>,
}

#[derive(Clone, Debug)]
pub struct TypedMatch {
    id: AgentId,
    aux: Vec<(Tree, Tree, Tree)>,
}

#[derive(Clone, Debug)]
pub struct Definition {
    left: UntypedMatch,
    right: UntypedMatch,
    net: Net,
}

#[derive(Clone, Debug)]
pub struct Declaration {
    agent: TypedMatch,
    intermediate: Vec<Tree>,
    r#type: UntypedMatch,
    net: Net,
}

#[derive(Clone, Debug, Default)]
struct ProgramBuilder {
    var_scope: BTreeMap<String, VarId>,
    agent_scope: BTreeMap<String, AgentId>,
    net: Net,
    agents: SlotMap<DefaultKey, ()>,
    declarations: Vec<Declaration>,
    definitions: Vec<Definition>,
    checks: Vec<(bool, Net)>,
}

impl Into<Tree> for UntypedMatch {
    fn into(self) -> Tree {
        Tree::Agent {
            id: self.id,
            aux: self.aux,
        }
    }
}

impl Tree {
    fn agent_id(&self) -> Option<AgentId> {
        match self {
            Tree::Agent { id, .. } => Some(id.clone()),
            Tree::Var { .. } => None,
        }
    }
}

impl ProgramBuilder {
    fn create_annotation_node(&mut self) {
        self.load_book(
            CodeParser::new("__ANN(a b) ~ __ANN(a b)")
                .parse_book()
                .unwrap(),
        );
    }
    fn get_ann_id(&mut self) -> AgentId {
        if let Some(a) = self.agent_scope.get("__ANN") {
            *a
        } else {
            self.create_annotation_node();
            *self.agent_scope.get("__ANN").unwrap()
        }
    }
    fn get_annotator_id(&mut self) -> AgentId {
        if let Some(a) = self.agent_scope.get("__ANNOTATOR") {
            *a
        } else {
            self.load_book(
                CodeParser::new("__ANNOTATOR(a) ~ __ANNOTATOR(a)")
                    .parse_book()
                    .unwrap(),
            );
            *self.agent_scope.get("__ANNOTATOR").unwrap()
        }
    }
    fn get_agent_id(&mut self, name: String) -> AgentId {
        *self
            .agent_scope
            .entry(name)
            .or_insert_with(|| self.agents.insert(()))
    }
    fn get_var_id(&mut self, name: String) -> VarId {
        *self
            .var_scope
            .entry(name)
            .or_insert_with(|| self.net.vars.insert(None))
    }
    fn load_untyped_match(&mut self, tree: syntax::UntypedMatch) -> UntypedMatch {
        UntypedMatch {
            id: self.get_agent_id(tree.name),
            aux: tree.aux.into_iter().map(|t| self.load_tree(t)).collect(),
        }
    }
    fn load_typed_match(&mut self, tree: syntax::TypedMatch) -> TypedMatch {
        TypedMatch {
            id: self.get_agent_id(tree.name),
            aux: tree
                .aux
                .into_iter()
                .map(|(a, b, c)| (self.load_tree(a), self.load_tree(b), self.load_tree(c)))
                .collect(),
        }
    }
    fn load_tree(&mut self, tree: syntax::Tree) -> Tree {
        match tree {
            syntax::Tree::Agent { name, aux } => Tree::Agent {
                id: self.get_agent_id(name),
                aux: aux.into_iter().map(|x| self.load_tree(x)).collect(),
            },
            syntax::Tree::Variable { name } => Tree::Var {
                id: self.get_var_id(name),
            },
            syntax::Tree::With { rest, redex } => {
                let t0 = self.load_tree(redex.0);
                let t1 = self.load_tree(redex.1);
                self.net.interactions.push((t0, t1));
                self.load_tree(*rest)
            }
        }
    }
    fn load_statement(&mut self, statement: Statement) {
        match statement {
            Statement::Decl(a, vars, t) => {
                let decl = Declaration {
                    agent: self.load_typed_match(a),
                    intermediate: vars.into_iter().map(|x| self.load_tree(x)).collect(),
                    r#type: self.load_untyped_match(t),
                    // note: relies on execution order
                    net: core::mem::take(&mut self.net),
                };
                self.add_decl_annotator_rule(&decl);
                self.declarations.push(decl);
            }
            Statement::Def(a, b) => {
                let def = Definition {
                    left: self.load_untyped_match(a),
                    right: self.load_untyped_match(b),
                    // note: relies on execution order
                    net: core::mem::take(&mut self.net),
                };
                self.definitions.push(def);
            }
            Statement::Check(positive, syntax::Net { interactions }) => {
                for (a, b) in interactions.into_iter() {
                    let a = self.load_tree(a);
                    let b = self.load_tree(b);
                    self.net.interactions.push((a, b))
                }
                self.checks.push((positive, core::mem::take(&mut self.net)))
            }
        }
        self.var_scope.clear();
    }
    fn add_decl_annotator_rule(&mut self, decl: &Declaration) {
        let def = Definition {
            left: UntypedMatch {
                id: self.get_annotator_id(),
                aux: vec![Tree::Agent {
                    id: self.get_ann_id(),
                    aux: vec![
                        Tree::Agent {
                            id: decl.agent.id,
                            aux: decl.agent.aux.iter().map(|x| x.1.clone()).collect(),
                        },
                        Tree::Agent {
                            id: decl.r#type.id,
                            aux: decl.r#type.aux.clone(),
                        },
                    ],
                }],
            },
            right: UntypedMatch {
                id: decl.agent.id,
                aux: decl
                    .agent
                    .aux
                    .iter()
                    .map(|x| Tree::Agent {
                        id: self.get_ann_id(),
                        aux: vec![x.0.clone(), x.2.clone()],
                    })
                    .collect(),
            },
            net: decl.net.clone(),
        };
        self.definitions.push(def);
    }
    fn load_book(&mut self, book: Vec<Statement>) {
        book.into_iter().for_each(|x| self.load_statement(x))
    }
    fn build_interaction_system(&mut self) -> Rc<InteractionSystem> {
        let mut isys = InteractionSystem::default();
        for i in self.definitions.iter() {
            assert!(isys
                .rules
                .entry(i.left.id)
                .or_default()
                .insert(
                    i.right.id,
                    InteractionRule {
                        left_ports: i.left.aux.clone(),
                        right_ports: i.right.aux.clone(),
                    }
                )
                .is_none());
            assert!(i.net.interactions.is_empty());
        }
        Rc::new(isys)
    }
    fn finish(mut self) -> Program {
        let system = self.build_interaction_system();
        let annotator_id = self.get_annotator_id();
        let ann_id = self.get_ann_id();

        Program {
            system,
            agent_scope: self.agent_scope,
            agents: self.agents,
            declarations: self.declarations,
            definitions: self.definitions,
            checks: self.checks,
            annotator_id,
            ann_id,
        }
    }
}

pub struct Program {
    pub system: Rc<InteractionSystem>,
    pub agent_scope: BTreeMap<String, AgentId>,
    pub agents: SlotMap<DefaultKey, ()>,
    pub declarations: Vec<Declaration>,
    pub definitions: Vec<Definition>,
    pub checks: Vec<(bool, Net)>,
    pub annotator_id: DefaultKey,
    pub ann_id: DefaultKey,
}

impl Program {
    fn typecheck_net(&self, mut net: Net) -> Result<(), String> {
        for (a, b) in core::mem::take(&mut net.interactions).into_iter() {
            let v = net.new_var();
            net.interactions.push((
                a,
                Tree::Agent {
                    id: self.annotator_id,
                    aux: vec![Tree::Var { id: v }],
                },
            ));
            net.interactions.push((
                b,
                Tree::Agent {
                    id: self.annotator_id,
                    aux: vec![Tree::Var { id: v }],
                },
            ));
        }
        net.system = self.system.clone();
        let mut gc = vec![];

        //print!("------------------------\n{}", net.show_net(&|key| self.lookup_agent(&key).unwrap_or("?".to_string()), &mut BTreeMap::new()));
        while let Some((is_stuck, (a, b))) = net
            .interactions
            .pop()
            .map(|x| (false, x))
            .or_else(|| net.stuck.pop().map(|x| (true, x)))
        {
            if is_stuck {
                let (a, b) = if b.agent_id().unwrap() == self.ann_id {
                    (b, a)
                } else {
                    (a, b)
                };
                if a.agent_id().unwrap() == self.ann_id {
                    let Tree::Agent { mut aux, .. } = a else {
                        unreachable!()
                    };
                    gc.push(aux.pop());
                    net.interact(aux.pop().unwrap(), b);
                } else {
                    return Err(format!(
                        "When typechecking net\n:\tUndefined Interaction:\n\t\t{ea} ~ {eb}",
                        ea = self.lookup_agent(&a.agent_id().unwrap()).unwrap(),
                        eb = self.lookup_agent(&b.agent_id().unwrap()).unwrap()
                    ));
                }
            } else {
                net.interact(a, b)
            }
            //print!("{}", net.show_net(&|key| self.lookup_agent(&key).unwrap_or("?".to_string()), &mut BTreeMap::new()));
        }
        if !net.stuck.is_empty() {
            Err("Had stuck interactions".to_string())
        } else {
            Ok(())
        }
    }
    fn check_well_typedness(&mut self) {
        for (should_check, net) in core::mem::take(&mut self.checks) {
            let res = self.typecheck_net(net);
            if !should_check {
                res.unwrap_err();
            } else {
                res.unwrap();
            }
        }
    }
    fn get_nth_instances(&self, t: AgentId, d: usize) -> impl Iterator<Item = AgentId> + Clone {
        let mut v = vec![];
        for i in &self.declarations {
            if i.intermediate.len() == d {
                if i.r#type.id == t {
                    v.push(i.agent.id);
                }
                if i.agent.id == t {
                    v.extend(self.get_nth_instances(i.r#type.id, d + 1));
                }
            }
        }
        v.into_iter()
    }
    fn lookup_agent(&self, id: &AgentId) -> Option<String> {
        self.agent_scope
            .iter()
            .find(|(_, v)| *v == id)
            .map(|x| x.0.to_string())
    }
    fn require_defined(&self, a: AgentId, b: AgentId) -> Result<(), String> {
        let defined = self
            .definitions
            .iter()
            .any(|x| x.left.id == a && x.right.id == b || x.left.id == b && x.right.id == a);
        if !defined {
            Err(format!(
                "Undefined interaction between {} and {}",
                self.lookup_agent(&a).unwrap(),
                self.lookup_agent(&b).unwrap(),
            ))
        } else {
            Ok(())
        }
    }
    pub fn check_completeness(&self) -> Result<(), String> {
        for def in &self.definitions {
            // Look for "child" interactions
            for (i, j) in iproduct!(
                self.get_nth_instances(def.left.id, 0),
                self.get_nth_instances(def.right.id, 0)
            ) {
                self.require_defined(i, j)?;
            }
        }
        Ok(())
    }
}

impl std::fmt::Display for Program {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Rules:\n")?;
        for (a, m) in &self.system.rules {
            for (b, _) in m {
                f.write_fmt(format_args!(
                    "\t{} ~ {}\n",
                    self.lookup_agent(a).unwrap(),
                    self.lookup_agent(b).unwrap()
                ))?
            }
        }
        f.write_str("Scope:\n")?;
        for (n, id) in &self.agent_scope {
            write!(f, "\t{:?} {:?}\n", n, id)?;
        }
        // todo print more things..
        Ok(())
    }
}

fn main() {
    let code = std::fs::read_to_string(std::env::args().skip(1).next().unwrap()).unwrap();
    let mut parser = CodeParser::new(&code);
    let ast = parser.parse_book();
    let Ok(ast) = ast else {
        eprintln!("{}", ast.unwrap_err());
        return;
    };
    let mut program = ProgramBuilder::default();
    program.load_book(ast);
    let mut program = program.finish();
    println!("{}", program);
    program.check_well_typedness();
    program.check_completeness().unwrap();
}
