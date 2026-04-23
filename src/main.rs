use std::io::{self, IsTerminal, Read, Write};

use inkwell::context::Context;
use kaleidoscope::frontend::{
    code_generation::Compiler,
    lexer::{Lexer, Token},
    parser::Parser,
};

fn main() {
    let context = Context::create();
    let mut compiler = Compiler::new(&context, "kaleidoscope");

    if io::stdin().is_terminal() {
        run_repl(&mut compiler);
    } else if let Err(err) = run_script(&mut compiler) {
        eprintln!("error: {err}");
    }
}

fn run_repl<'ctx>(compiler: &mut Compiler<'ctx>) {
    println!("enter 'def', 'extern', or an expression");
    println!(":dump");

    let stdin = io::stdin();
    let mut line = String::new();

    loop {
        print!("ready> ");
        if let Err(err) = io::stdout().flush() {
            eprintln!("error: failed to flush stdout: {err}");
            return;
        }

        line.clear();
        match stdin.read_line(&mut line) {
            Ok(0) => return,
            Ok(_) => {
                if let Err(err) = handle_input_line(compiler, line.trim()) {
                    eprintln!("error: {err}");
                }
            }
            Err(err) => {
                eprintln!("error: failed to read stdin: {err}");
                return;
            }
        }
    }
}

fn run_script<'ctx>(compiler: &mut Compiler<'ctx>) -> Result<(), String> {
    let mut source = String::new();
    io::stdin()
        .read_to_string(&mut source)
        .map_err(|e| format!("failed to read script input: {e}"))?;

    for line in source.lines() {
        let trimmed = line.trim();
        handle_input_line(compiler, trimmed)?;
    }

    Ok(())
}

fn handle_input_line<'ctx>(compiler: &mut Compiler<'ctx>, line: &str) -> Result<(), String> {
    if line.is_empty() {
        return Ok(());
    }

    if line == ":dump" {
        compiler.module.print_to_stderr();
        return Ok(());
    }

    let mut parser = Parser::new(Lexer::new(line.chars()));
    parser.get_next_token();

    match parser.cur_tok() {
        Token::Eof => Ok(()),
        Token::Char(';') => {
            parser.get_next_token();
            Ok(())
        }
        Token::Def => {
            let function = parser.parse_definition()?;
            ensure_fully_parsed(&parser)?;
            compiler.emit_function(&function)?;
            println!("parsed and emitted definition '{}'", function.0.0);
            Ok(())
        }
        Token::Extern => {
            let prototype = parser.parse_extern()?;
            ensure_fully_parsed(&parser)?;
            compiler.emit_prototype(&prototype)?;
            println!("parsed extern '{}'", prototype.0);
            Ok(())
        }
        _ => {
            clear_previous_anon_expr(compiler);
            let function = parser.parse_top_level_expr()?;
            ensure_fully_parsed(&parser)?;
            compiler.emit_function(&function)?;
            let result = compiler.run_anon_expr()?;
            println!("=> {result}");
            Ok(())
        }
    }
}

fn ensure_fully_parsed<I>(parser: &Parser<I>) -> Result<(), String>
where
    I: Iterator<Item = char>,
{
    if *parser.cur_tok() == Token::Eof || *parser.cur_tok() == Token::Char(';') {
        Ok(())
    } else {
        Err(format!(
            "unexpected trailing input starting with {}",
            token_to_string(parser.cur_tok())
        ))
    }
}

fn token_to_string(tok: &Token) -> String {
    match tok {
        Token::Eof => "end-of-input".to_string(),
        Token::Def => "'def'".to_string(),
        Token::Extern => "'extern'".to_string(),
        Token::Identifier(id) => format!("identifier '{id}'"),
        Token::Number(n) => format!("number '{n}'"),
        Token::Char(c) => format!("'{c}'"),
    }
}

fn clear_previous_anon_expr<'ctx>(compiler: &mut Compiler<'ctx>) {
    if let Some(function) = compiler.module.get_function("__anon_expr") {
        unsafe { function.delete() };
    }
}
