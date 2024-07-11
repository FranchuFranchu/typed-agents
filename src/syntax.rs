use TSPL::Parser;

#[derive(Debug, Clone)]
pub enum Tree {
    Agent {
        name: String,
        aux: Vec<Tree>,
    },
    Variable {
        name: String,
    },
    With {
        rest: Box<Tree>,
        redex: Box<(Tree, Tree)>,
    },
}

#[derive(Debug, Clone)]
pub struct TypedMatch {
    pub name: String,
    pub aux: Vec<(Tree, Tree, Tree)>,
}
#[derive(Debug, Clone)]
pub struct UntypedMatch {
    pub name: String,
    pub aux: Vec<Tree>,
}
#[derive(Debug, Clone)]
pub struct Net {
    pub interactions: Vec<(Tree, Tree)>,
}

#[derive(Debug, Clone)]
pub enum Statement {
    Decl(TypedMatch, Vec<Tree>, UntypedMatch),
    Def(UntypedMatch, UntypedMatch),
    Check(bool, Net),
}

pub struct CodeParser<'i> {
    input: &'i str,
    index: usize,
}
impl<'i> Parser<'i> for CodeParser<'i> {
    fn input(&mut self) -> &'i str {
        &self.input
    }
    fn index(&mut self) -> &mut usize {
        &mut self.index
    }
}
impl<'i> CodeParser<'i> {
    pub fn new(input: &'i str) -> Self {
        Self { input, index: 0 }
    }
}

impl<'i> CodeParser<'i> {
    fn skip_trivia(&mut self) {
        while let Some(c) = self.peek_one() {
            if c.is_ascii_whitespace() {
                self.advance_one();
                continue;
            }
            if c == ';' {
                while let Some(c) = self.peek_one() {
                    if c != '\n' {
                        self.advance_one();
                    } else {
                        break;
                    }
                }
                self.advance_one(); // Skip the newline character as well
                continue;
            }
            break;
        }
    }

    fn parse_statement(&mut self) -> Result<Statement, String> {
        let index = self.index;
        self.skip_trivia();
        if self.peek_many(5) == Some("check") {
            self.consume("check")?;
            self.skip_trivia();
            let positive = match self.parse_name()?.as_ref() {
                "yes" => true,
                "no" => false,
                _ => return Err("Expected yes or no".to_string()),
            };
            let net = self.parse_net()?;
            return Ok(Statement::Check(positive, net));
        }
        let untyped_match = self.parse_untyped_match();
        self.skip_trivia();
        if let Ok(untyped_match) = untyped_match.clone()
            && self.peek_one() == Some('~')
        {
            self.consume("~")?;
            let a = self.parse_untyped_match()?;
            return Ok(Statement::Def(untyped_match, a));
        }
        self.index = index;
        let typed_match = self.parse_typed_match();
        self.skip_trivia();
        if let Ok(typed_match) = typed_match.clone()
            && self.peek_one() == Some(':')
        {
            self.consume(":")?;
            let mut vars = vec![];
            self.skip_trivia();
            let mut index = self.index;
            let mut tree = self.parse_tree();
            self.skip_trivia();
            while let Ok(next_tree) = tree
                && self.peek_one() == Some(':')
            {
                vars.push(next_tree);
                self.consume(":")?;
                self.skip_trivia();
                index = self.index;
                tree = self.parse_tree();
                self.skip_trivia();
            }
            self.index = index;
            let end = self.parse_untyped_match()?;
            return Ok(Statement::Decl(typed_match, vars, end));
        }
        self.index = index;
        self.expected("Expected typed pattern match or untyped pattern match.")?
    }
    pub fn parse_book(&mut self) -> Result<Vec<Statement>, String> {
        self.skip_trivia();
        let mut book = vec![];
        while self.peek_one().is_some() {
            book.push(self.parse_statement()?);
            self.skip_trivia();
        }
        Ok(book)
    }
    fn is_name_char(c: char) -> bool {
        return !c.is_whitespace() && !c.is_control() && !":=~()".contains(c);
    }
    fn parse_var(&mut self) -> Result<String, String> {
        self.skip_trivia();
        if self.peek_one().is_some_and(|x| x.is_lowercase()) {
            self.parse_name()
        } else {
            Err("Not a var name char".to_string())
        }
    }
    fn parse_name(&mut self) -> Result<String, String> {
        self.skip_trivia();
        let name = self.take_while(|c| Self::is_name_char(c));
        if name.is_empty() {
            self.expected("name")
        } else {
            Ok(name.to_owned())
        }
    }
    fn parse_untyped_match(&mut self) -> Result<UntypedMatch, String> {
        self.skip_trivia();
        let name = self.parse_name()?;
        self.skip_trivia();
        let args = if self.peek_one() == Some('(') {
            self.consume("(")?;
            let mut args = vec![];
            self.skip_trivia();
            while self.peek_one() != Some(')') {
                args.push(self.parse_tree()?);
                self.skip_trivia();
            }
            self.consume(")")?;
            args
        } else {
            vec![]
        };
        Ok(UntypedMatch { name, aux: args })
    }
    fn parse_typed_match(&mut self) -> Result<TypedMatch, String> {
        self.skip_trivia();
        let name = self.parse_name()?;
        self.skip_trivia();
        let args = if self.peek_one() == Some('(') {
            self.consume("(")?;
            let mut args = vec![];
            self.skip_trivia();
            while self.peek_one() != Some(')') {
                let from = self.parse_tree()?;
                self.skip_trivia();
                self.consume("->")?;
                let to = self.parse_tree()?;
                self.skip_trivia();
                self.consume(":")?;
                let r#type = self.parse_tree()?;
                args.push((from, to, r#type));
                self.skip_trivia();
            }
            self.consume(")")?;
            args
        } else {
            vec![]
        };
        Ok(TypedMatch { name, aux: args })
    }
    fn parse_tree(&mut self) -> Result<Tree, String> {
        self.skip_trivia();
        let name = self.parse_name()?;
        let res = if name.chars().next().unwrap().is_lowercase() {
            // Variable
            Tree::Variable { name }
        } else {
            // Agent
            self.skip_trivia();
            let args = if self.peek_one() == Some('(') {
                self.consume("(")?;
                let mut args = vec![];
                self.skip_trivia();
                while self.peek_one() != Some(')') {
                    args.push(self.parse_tree()?);
                    self.skip_trivia();
                }
                self.consume(")")?;
                args
            } else {
                vec![]
            };
            Tree::Agent { name, aux: args }
        };
        self.skip_trivia();
        if self.peek_many(4) == Some("with") {
            self.consume("with")?;
            let l = self.parse_tree()?;
            self.skip_trivia();
            self.consume("~")?;
            let r = self.parse_tree()?;
            Ok(Tree::With {
                rest: Box::new(res),
                redex: Box::new((l, r)),
            })
        } else {
            Ok(res)
        }
    }
    fn parse_net(&mut self) -> Result<Net, String> {
        let a = self.parse_tree()?;
        self.skip_trivia();
        self.consume("~")?;
        let b = self.parse_tree()?;
        Ok(Net {
            interactions: vec![(a, b)],
        })
    }
}
