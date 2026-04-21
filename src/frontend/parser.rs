use crate::frontend::lexer::{Lexer, Token};

#[allow(dead_code, unused_allocation)]
#[derive(PartialEq, Debug)]
pub enum ExprAST {
    Number(f64),

    /// Variable - Expression enum for referencing a variable, like "a".
    Variable(String),

    // Box<T> cus recurisive type, so rust need
    // to know the compile time size.
    // Binary - Expression enum for a binary operator.
    Binary(char, Box<ExprAST>, Box<ExprAST>),

    /// Call - Expression enum for function calls.
    Call(String, Vec<ExprAST>),
}

pub struct Parser<I>
where
    I: Iterator<Item = char>,
{
    lexer: Lexer<I>,
    cur_tok: Option<Token>,
}

type ParseResult<T> = Result<T, String>;

impl<I> Parser<I>
where
    I: Iterator<Item = char>,
{
    pub fn new(lexer: Lexer<I>) -> Self {
        Self {
            lexer,
            cur_tok: None,
        }
    }
    /// cur_tok/get_next_token - Provide a simple token buffer.  cur_tok is the current
    /// token the parser is looking at.  get_next_token reads another token from the
    /// lexer and updates cur_tok with its results.
    pub fn get_next_token(&mut self) {
        self.cur_tok = Some(self.lexer.gettok());
    }

    pub fn cur_tok(&self) -> &Token {
        self.cur_tok.as_ref().expect("Expected cur_token")
    }

    pub fn parse_num_expr(&mut self) -> ParseResult<ExprAST> {
        match *self.cur_tok() {
            Token::Number(num) => {
                self.get_next_token();
                Ok(ExprAST::Number(num))
            }
            _ => unreachable!(),
        }
    }

    pub fn parse_paren_expr(&mut self) -> ParseResult<ExprAST> {
        self.get_next_token();
        let v = self.parse_expression()?;

        if *self.cur_tok() == Token::Char(')') {
            self.get_next_token();
            Ok(v)
        } else {
            Err("Expected ')'".into())
        }
    }

    fn parse_identifier_expr(&mut self) -> ParseResult<ExprAST> {
        let id_name = match self.cur_tok.take() {
            Some(Token::Identifier(id)) => {
                self.get_next_token();
                id
            }
            _ => unreachable!(),
        };

        if *self.cur_tok() != Token::Char('(') {
            Ok(ExprAST::Variable(id_name))
        } else {
            self.get_next_token();

            let mut args: Vec<ExprAST> = Vec::new();

            if *self.cur_tok() != Token::Char(')') {
                loop {
                    let arg = self.parse_expression()?;
                    args.push(arg);

                    if *self.cur_tok() == Token::Char(')') {
                        break;
                    }

                    if *self.cur_tok() != Token::Char(',') {
                        return Err("Expected ')' or ',' in argument list".into());
                    }

                    self.get_next_token();
                }
            }

            assert_eq!(*self.cur_tok(), Token::Char(')'));
            self.get_next_token();

            Ok(ExprAST::Call(id_name, args))
        }
    }

    fn parse_primary(&mut self) -> ParseResult<ExprAST> {
        match *self.cur_tok() {
            Token::Number(_) => self.parse_num_expr(),
            Token::Char('(') => self.parse_paren_expr(),
            Token::Identifier(_) => self.parse_identifier_expr(),
            _ => Err("unknown token when expecting an expression".into()),
        }
    }

    fn parse_bin_op_rhs(&mut self, expr_prec: isize, mut lhs: ExprAST) -> ParseResult<ExprAST> {
        loop {
            let tok_prec = get_tok_precedence(self.cur_tok());

            // Not a binary operator or precedence is too small.
            if tok_prec < expr_prec {
                return Ok(lhs);
            }

            let binop = match self.cur_tok.take() {
                Some(Token::Char(c)) => {
                    // Eat binary operator.
                    self.get_next_token();
                    c
                }
                _ => unreachable!(),
            };

            // lhs BINOP1 rhs BINOP2 remrhs
            //     ^^^^^^     ^^^^^^
            //     tok_prec   next_prec
            //
            // In case BINOP1 has higher precedence, we are done here and can build a 'Binary' AST
            // node between 'lhs' and 'rhs'.
            //
            // example:
            //
            // `2(lhs) +(BINOP1) 3(rhs) -(BINOP2) 4(...remrhs)`
            // equal precedence, hence can the Binary expressiona can be made
            // prev = (BINOP1, 2, 3) and next = (BINOP2, prev, 4).
            //
            // In case BINOP2 has higher precedence, we take 'rhs' as 'lhs' and recurse into the
            // 'remrhs' expression first.
            //
            // example:
            //
            // `2(lhs) +(BINOP1) 3(rhs) *(BINOP2) 4(...remrhs)`
            // '*' has higher precedence here than '+', so making prev= (BINOP1, 2, 3)
            // and (BINOP2, prev, 4) will give a wrong output (20).
            //
            // instead we recurisively cover `remrhs` and make a
            // next = (BINOP2, 3, 4) and prev = (BINOP1, 2, next)
            // to get the right output (14).

            // Parse primary expression after binary operator.
            let mut rhs = self.parse_primary()?;

            let next_prec = get_tok_precedence(self.cur_tok());
            if tok_prec < next_prec {
                // BINOP2 has higher precedence thatn BINOP1, recurse into 'remhs'.
                rhs = self.parse_bin_op_rhs(tok_prec + 1, rhs)?
            }

            lhs = ExprAST::Binary(binop, Box::new(lhs), Box::new(rhs));
        }
    }

    pub fn parse_prototype(&mut self) -> ParseResult<PrototypeAST> {
        let id_name = match self.cur_tok.take() {
            Some(Token::Identifier(id)) => {
                self.get_next_token();
                id
            }
            other => {
                self.cur_tok = other;
                return Err("Expected function name in prototype".into());
            }
        };

        if *self.cur_tok() != Token::Char('(') {
            return Err("Expected a '(' in prototype".into());
        }

        let mut args: Vec<String> = Vec::new();

        loop {
            self.get_next_token();
            match self.cur_tok.take() {
                Some(Token::Identifier(arg)) => args.push(arg),
                // someFunction(arg1 , arg2) {}
                //                   ^
                //                   skip
                Some(Token::Char(',')) => {}
                other => {
                    self.cur_tok = other;
                    break;
                }
            }
        }

        if *self.cur_tok() != Token::Char(')') {
            return Err("Expected ')' in prototype".into());
        }
        self.get_next_token();

        Ok(PrototypeAST(id_name, args))
    }

    pub fn parse_expression(&mut self) -> ParseResult<ExprAST> {
        let lhs = self.parse_primary()?;
        self.parse_bin_op_rhs(0, lhs)
    }

    pub fn parse_function(&mut self) -> ParseResult<FunctionAST> {
        self.get_next_token();

        let proto = self.parse_prototype()?;
        let expr = self.parse_expression()?;

        Ok(FunctionAST(proto, expr))
    }

    pub fn parse_extern(&mut self) -> ParseResult<PrototypeAST> {
        self.get_next_token();
        self.parse_prototype()
    }

    pub fn parse_top_level_expr(&mut self) -> ParseResult<FunctionAST> {
        let e = self.parse_expression()?;
        let proto = PrototypeAST("__anon_expr".into(), Vec::new());
        Ok(FunctionAST(proto, e))
    }
}
fn get_tok_precedence(tok: &Token) -> isize {
    match tok {
        Token::Char('<') => 10,
        Token::Char('+') => 20,
        Token::Char('-') => 20,
        Token::Char('*') => 40,
        _ => -1,
    }
}

pub struct PrototypeAST(pub String, pub Vec<String>);

pub struct FunctionAST(pub PrototypeAST, pub ExprAST);

impl PrototypeAST {
    pub fn new(name: String, args: Vec<String>) -> Self {
        Self(name, args)
    }

    pub fn get_name(&self) -> &String {
        &self.0
    }
}

#[cfg(test)]
mod test {}
