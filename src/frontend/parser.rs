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

        // not a function
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

    pub fn parse_definition(&mut self) -> ParseResult<FunctionAST> {
        self.get_next_token();
        let proto = self.parse_prototype()?;
        let expr = self.parse_expression()?;

        Ok(FunctionAST(proto, expr))
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

#[derive(Debug, PartialEq)]
pub struct PrototypeAST(pub String, pub Vec<String>);

#[derive(Debug, PartialEq)]
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
mod test {
    use super::{ExprAST, FunctionAST, Parser, PrototypeAST};
    use crate::frontend::lexer::Lexer;

    fn make_parser(input: &str) -> Parser<std::str::Chars<'_>> {
        let l = Lexer::new(input.chars());
        let mut p = Parser::new(l);
        p.get_next_token();
        p
    }

    #[test]
    fn numeric_literal_parses_correctly() {
        let mut p = make_parser("13.37");
        assert_eq!(p.parse_num_expr(), Ok(ExprAST::Number(13.37f64)));
    }

    #[test]
    fn bare_identifier_yields_variable_node() {
        let mut p = make_parser("foop");
        assert_eq!(
            p.parse_identifier_expr(),
            Ok(ExprAST::Variable("foop".into()))
        );
    }

    #[test]
    fn primary_dispatch_routes_to_correct_variants() {
        let mut p = make_parser("1337 foop \n bla(123)");
        assert_eq!(p.parse_primary(), Ok(ExprAST::Number(1337f64)));
        assert_eq!(p.parse_primary(), Ok(ExprAST::Variable("foop".into())));
        assert_eq!(
            p.parse_primary(),
            Ok(ExprAST::Call("bla".into(), vec![ExprAST::Number(123f64)]))
        );
    }

    #[test]
    fn left_assoc_binop_builds_left_leaning_tree() {
        //       -
        //      / \
        //     +   c
        //    / \
        //   a   b
        let mut p = make_parser("a + b - c");
        let ab = ExprAST::Binary(
            '+',
            Box::new(ExprAST::Variable("a".into())),
            Box::new(ExprAST::Variable("b".into())),
        );
        let abc = ExprAST::Binary('-', Box::new(ab), Box::new(ExprAST::Variable("c".into())));
        assert_eq!(p.parse_expression(), Ok(abc));
    }

    #[test]
    fn higher_prec_rhs_builds_right_leaning_tree() {
        //       +
        //      / \
        //     a   *
        //        / \
        //       b   c
        let mut p = make_parser("a + b * c");
        let bc = ExprAST::Binary(
            '*',
            Box::new(ExprAST::Variable("b".into())),
            Box::new(ExprAST::Variable("c".into())),
        );
        let abc = ExprAST::Binary('+', Box::new(ExprAST::Variable("a".into())), Box::new(bc));
        assert_eq!(p.parse_expression(), Ok(abc));
    }

    #[test]
    fn prototype_with_two_args_parses_correctly() {
        let mut p = make_parser("foo(a,b)");
        assert_eq!(
            p.parse_prototype(),
            Ok(PrototypeAST("foo".into(), vec!["a".into(), "b".into()]))
        );
    }

    #[test]
    fn function_def_with_binary_body_parses_correctly() {
        let mut p = make_parser("def bar( arg0 , arg1 ) arg0 + arg1");
        let proto = PrototypeAST("bar".into(), vec!["arg0".into(), "arg1".into()]);
        let body = ExprAST::Binary(
            '+',
            Box::new(ExprAST::Variable("arg0".into())),
            Box::new(ExprAST::Variable("arg1".into())),
        );
        assert_eq!(p.parse_definition(), Ok(FunctionAST(proto, body)));
    }

    #[test]
    fn extern_decl_with_no_args_parses_correctly() {
        let mut p = make_parser("extern baz()");
        assert_eq!(p.parse_extern(), Ok(PrototypeAST("baz".into(), vec![])));
    }

    fn init(input: &str) -> Parser<std::str::Chars<'_>> {
        let mut p = Parser::new(Lexer::new(input.chars()));
        p.get_next_token();
        p
    }

    #[test]
    fn num_integer_value() {
        let mut p = init("42");
        assert_eq!(p.parse_num_expr(), Ok(ExprAST::Number(42.0)));
    }

    #[test]
    fn num_zero() {
        let mut p = init("0");
        assert_eq!(p.parse_num_expr(), Ok(ExprAST::Number(0.0)));
    }

    #[test]
    fn num_negative_float() {
        // Unary minus is not a token the lexer sees '-' separately
        let mut p = init("3.13");
        assert_eq!(p.parse_num_expr(), Ok(ExprAST::Number(3.13)));
    }

    #[test]
    fn num_large_value() {
        let mut p = init("999999.999");
        assert_eq!(p.parse_num_expr(), Ok(ExprAST::Number(999999.999)));
    }

    #[test]
    fn identifier_single_char_var() {
        let mut p = init("x");
        assert_eq!(p.parse_identifier_expr(), Ok(ExprAST::Variable("x".into())));
    }

    #[test]
    fn identifier_underscore_name() {
        let mut p = init("_my_var");
        assert_eq!(
            p.parse_identifier_expr(),
            Ok(ExprAST::Variable("_my_var".into()))
        );
    }

    #[test]
    fn call_with_zero_args() {
        let mut p = init("rand()");
        assert_eq!(
            p.parse_identifier_expr(),
            Ok(ExprAST::Call("rand".into(), vec![]))
        );
    }

    #[test]
    fn call_with_single_numeric_arg() {
        let mut p = init("sqrt(4)");
        assert_eq!(
            p.parse_identifier_expr(),
            Ok(ExprAST::Call("sqrt".into(), vec![ExprAST::Number(4.0)]))
        );
    }

    #[test]
    fn call_with_multiple_numeric_args() {
        let mut p = init("add(1, 2, 3)");
        assert_eq!(
            p.parse_identifier_expr(),
            Ok(ExprAST::Call(
                "add".into(),
                vec![
                    ExprAST::Number(1.0),
                    ExprAST::Number(2.0),
                    ExprAST::Number(3.0),
                ]
            ))
        );
    }

    #[test]
    fn call_with_variable_args() {
        let mut p = init("max(x, y)");
        assert_eq!(
            p.parse_identifier_expr(),
            Ok(ExprAST::Call(
                "max".into(),
                vec![ExprAST::Variable("x".into()), ExprAST::Variable("y".into()),]
            ))
        );
    }

    #[test]
    fn call_arg_list_missing_closing_paren_is_error() {
        let mut p = init("foo(a, b");
        // Will hit EOF/unknown token instead of ',' or ')', expect error
        assert!(p.parse_identifier_expr().is_err());
    }

    #[test]
    fn paren_wrapping_number_unwraps_cleanly() {
        let mut p = init("(7)");
        assert_eq!(p.parse_paren_expr(), Ok(ExprAST::Number(7.0)));
    }

    #[test]
    fn paren_wrapping_variable_unwraps_cleanly() {
        let mut p = init("(alpha)");
        assert_eq!(p.parse_paren_expr(), Ok(ExprAST::Variable("alpha".into())));
    }

    #[test]
    fn paren_wrapping_binary_expr() {
        let mut p = init("(a + b)");
        let expected = ExprAST::Binary(
            '+',
            Box::new(ExprAST::Variable("a".into())),
            Box::new(ExprAST::Variable("b".into())),
        );
        assert_eq!(p.parse_paren_expr(), Ok(expected));
    }

    #[test]
    fn paren_unclosed_returns_error() {
        let mut p = init("(a + b");
        assert!(p.parse_paren_expr().is_err());
    }

    // precedence climbing

    #[test]
    fn expr_single_number() {
        let mut p = init("5");
        assert_eq!(p.parse_expression(), Ok(ExprAST::Number(5.0)));
    }

    #[test]
    fn expr_single_variable() {
        let mut p = init("z");
        assert_eq!(p.parse_expression(), Ok(ExprAST::Variable("z".into())));
    }

    #[test]
    fn expr_mul_has_higher_prec_than_add() {
        // a * b + c  =>  (a*b) + c   (left-leaning at '+')
        //       +
        //      / \
        //     *   c
        //    / \
        //   a   b
        let mut p = init("a * b + c");
        let ab = ExprAST::Binary(
            '*',
            Box::new(ExprAST::Variable("a".into())),
            Box::new(ExprAST::Variable("b".into())),
        );
        let expected = ExprAST::Binary('+', Box::new(ab), Box::new(ExprAST::Variable("c".into())));
        assert_eq!(p.parse_expression(), Ok(expected));
    }

    #[test]
    fn expr_mul_has_higher_prec_than_sub() {
        // a - b * c  =>  a - (b*c)
        let mut p = init("a - b * c");
        let bc = ExprAST::Binary(
            '*',
            Box::new(ExprAST::Variable("b".into())),
            Box::new(ExprAST::Variable("c".into())),
        );
        let expected = ExprAST::Binary('-', Box::new(ExprAST::Variable("a".into())), Box::new(bc));
        assert_eq!(p.parse_expression(), Ok(expected));
    }

    #[test]
    fn expr_less_than_lowest_prec() {
        // a < b + c  =>  a < (b+c)
        let mut p = init("a < b + c");
        let bc = ExprAST::Binary(
            '+',
            Box::new(ExprAST::Variable("b".into())),
            Box::new(ExprAST::Variable("c".into())),
        );
        let expected = ExprAST::Binary('<', Box::new(ExprAST::Variable("a".into())), Box::new(bc));
        assert_eq!(p.parse_expression(), Ok(expected));
    }

    #[test]
    fn expr_chained_mul_left_assoc() {
        // a * b * c  =>  (a*b) * c
        let mut p = init("a * b * c");
        let ab = ExprAST::Binary(
            '*',
            Box::new(ExprAST::Variable("a".into())),
            Box::new(ExprAST::Variable("b".into())),
        );
        let expected = ExprAST::Binary('*', Box::new(ab), Box::new(ExprAST::Variable("c".into())));
        assert_eq!(p.parse_expression(), Ok(expected));
    }

    #[test]
    fn expr_parens_override_precedence() {
        // (a + b) * c  =>  * at root
        let mut p = init("(a + b) * c");
        let ab = ExprAST::Binary(
            '+',
            Box::new(ExprAST::Variable("a".into())),
            Box::new(ExprAST::Variable("b".into())),
        );
        let expected = ExprAST::Binary('*', Box::new(ab), Box::new(ExprAST::Variable("c".into())));
        assert_eq!(p.parse_expression(), Ok(expected));
    }

    #[test]
    fn expr_call_inside_binary() {
        let mut p = init("foo(x) + 1");
        let call = ExprAST::Call("foo".into(), vec![ExprAST::Variable("x".into())]);
        let expected = ExprAST::Binary('+', Box::new(call), Box::new(ExprAST::Number(1.0)));
        assert_eq!(p.parse_expression(), Ok(expected));
    }

    #[test]
    fn expr_nested_call_args() {
        let mut p = init("outer(inner(x), y)");
        let inner = ExprAST::Call("inner".into(), vec![ExprAST::Variable("x".into())]);
        let expected = ExprAST::Call("outer".into(), vec![inner, ExprAST::Variable("y".into())]);
        assert_eq!(p.parse_expression(), Ok(expected));
    }

    #[test]
    fn expr_unknown_token_returns_error() {
        let mut p = init("@bad");
        assert!(p.parse_expression().is_err());
    }

    // parse_prototype

    #[test]
    fn prototype_no_args() {
        let mut p = init("noargs()");
        assert_eq!(
            p.parse_prototype(),
            Ok(PrototypeAST("noargs".into(), vec![]))
        );
    }

    #[test]
    fn prototype_single_arg() {
        let mut p = init("identity(x)");
        assert_eq!(
            p.parse_prototype(),
            Ok(PrototypeAST("identity".into(), vec!["x".into()]))
        );
    }

    #[test]
    fn prototype_three_args() {
        let mut p = init("clamp(val, lo, hi)");
        assert_eq!(
            p.parse_prototype(),
            Ok(PrototypeAST(
                "clamp".into(),
                vec!["val".into(), "lo".into(), "hi".into()]
            ))
        );
    }

    #[test]
    fn prototype_missing_open_paren_is_error() {
        let mut p = init("noparen x)");
        assert!(p.parse_prototype().is_err());
    }

    #[test]
    fn prototype_missing_close_paren_is_error() {
        let mut p = init("noparen(x");
        assert!(p.parse_prototype().is_err());
    }

    #[test]
    fn prototype_non_identifier_name_is_error() {
        let mut p = init("123(a)");
        assert!(p.parse_prototype().is_err());
    }

    // parse_definition

    #[test]
    fn definition_single_arg_numeric_body() {
        let mut p = init("def double(x) x * 2");
        let proto = PrototypeAST("double".into(), vec!["x".into()]);
        let body = ExprAST::Binary(
            '*',
            Box::new(ExprAST::Variable("x".into())),
            Box::new(ExprAST::Number(2.0)),
        );
        assert_eq!(p.parse_definition(), Ok(FunctionAST(proto, body)));
    }
    #[test]
    fn definition_body_is_function_call() {
        let mut p = init("def wrapper(x) sqrt(x)");
        let proto = PrototypeAST("wrapper".into(), vec!["x".into()]);
        let body = ExprAST::Call("sqrt".into(), vec![ExprAST::Variable("x".into())]);
        assert_eq!(p.parse_definition(), Ok(FunctionAST(proto, body)));
    }

    #[test]
    fn definition_three_arg_complex_body() {
        let mut p = init("def mad(a, b, c) a * b + c");
        let proto = PrototypeAST("mad".into(), vec!["a".into(), "b".into(), "c".into()]);
        let ab = ExprAST::Binary(
            '*',
            Box::new(ExprAST::Variable("a".into())),
            Box::new(ExprAST::Variable("b".into())),
        );
        let body = ExprAST::Binary('+', Box::new(ab), Box::new(ExprAST::Variable("c".into())));
        assert_eq!(p.parse_definition(), Ok(FunctionAST(proto, body)));
    }

    // parse_extern

    #[test]
    fn extern_single_arg() {
        let mut p = init("extern sin(x)");
        assert_eq!(
            p.parse_extern(),
            Ok(PrototypeAST("sin".into(), vec!["x".into()]))
        );
    }

    #[test]
    fn extern_two_args() {
        let mut p = init("extern pow(base, exp)");
        assert_eq!(
            p.parse_extern(),
            Ok(PrototypeAST(
                "pow".into(),
                vec!["base".into(), "exp".into()]
            ))
        );
    }

    #[test]
    fn extern_missing_name_is_error() {
        // 'extern (x)' number where name expected
        let mut p = init("extern (x)");
        assert!(p.parse_extern().is_err());
    }

    // parse_top_level_expr

    #[test]
    fn top_level_number_becomes_anon_fn() {
        let mut p = init("42");
        let result = p.parse_top_level_expr().unwrap();
        assert_eq!(result.0, PrototypeAST("__anon_expr".into(), vec![]));
        assert_eq!(result.1, ExprAST::Number(42.0));
    }

    #[test]
    fn top_level_binary_expr_becomes_anon_fn() {
        let mut p = init("x + y");
        let result = p.parse_top_level_expr().unwrap();
        assert_eq!(result.0.0, "__anon_expr");
        assert_eq!(
            result.1,
            ExprAST::Binary(
                '+',
                Box::new(ExprAST::Variable("x".into())),
                Box::new(ExprAST::Variable("y".into())),
            )
        );
    }

    // PrototypeAST helpers

    #[test]
    fn prototype_get_name_returns_correct_string() {
        let proto = PrototypeAST::new("myFunc".into(), vec!["a".into()]);
        assert_eq!(proto.get_name(), "myFunc");
    }

    #[test]
    fn prototype_new_stores_args_in_order() {
        let args = vec!["x".into(), "y".into(), "z".into()];
        let proto = PrototypeAST::new("f".into(), args.clone());
        assert_eq!(proto.1, args);
    }

    // ExprAST structural equality

    #[test]
    fn expr_ast_number_equality() {
        assert_eq!(ExprAST::Number(1.0), ExprAST::Number(1.0));
        assert_ne!(ExprAST::Number(1.0), ExprAST::Number(2.0));
    }

    #[test]
    fn expr_ast_variable_equality() {
        assert_eq!(ExprAST::Variable("a".into()), ExprAST::Variable("a".into()));
        assert_ne!(ExprAST::Variable("a".into()), ExprAST::Variable("b".into()));
    }

    #[test]
    fn expr_ast_call_equality() {
        let c1 = ExprAST::Call("f".into(), vec![ExprAST::Number(1.0)]);
        let c2 = ExprAST::Call("f".into(), vec![ExprAST::Number(1.0)]);
        let c3 = ExprAST::Call("g".into(), vec![ExprAST::Number(1.0)]);
        assert_eq!(c1, c2);
        assert_ne!(c1, c3);
    }
}
