pub mod error;
pub mod parser;
pub mod semck;
pub mod sym;
pub mod ty;
pub mod utils;
pub mod vm;

#[cfg(not(test))]
use std::process::exit;

use std::path::Path;

use crate::vm::VM;
use crate::parser::ast::{self, Ast};
use crate::parser::lexer::reader::Reader;
use crate::parser::parser::Parser;

use std::default::Default;
use docopt::Docopt;

pub fn parse() -> Args {
    Docopt::new(USAGE)
        .and_then(|d| d.decode())
        .unwrap_or_else(|e| e.exit())
}

// Write the Docopt usage string.
static USAGE: &'static str = "
Usage: llama <file>
       llama (--version | --help)

Options:
    -h, --help              Shows this text.
    --version               Shows version.
";

#[derive(Debug, RustcDecodable)]
pub struct Args {
    pub arg_argument: Option<Vec<String>>,
    pub arg_file: String,
    pub flag_version: bool,
}

impl Default for Args {
    fn default() -> Args {
        Args {
            arg_argument: None,
            arg_file: "".into(),
            flag_version: false,
        }
    }
}

pub fn start() -> i32 {
    let args = parse();

    if args.flag_version {
        println!("llama v0.01");
        return 0;
    }

    let mut ast = Ast::new();
    let empty = Ast::new();
    let mut vm = VM::new(&empty);

    let arg_file = args.arg_file.clone();
    let path = Path::new(&arg_file);

    let res = if path.is_file() {
        parse_file(&arg_file, &mut vm, &mut ast)
    } else {
        println!("file `{}` does not exist.", &arg_file);
        Err(1)
    };

    if let Err(code) = res {
        return code;
    }

    vm.ast = &ast;
    
    ast::dump::dump(&vm.ast, &vm.interner);
    0
}


fn parse_file(filename: &str, vm: &mut VM, ast: &mut Ast) -> Result<(), i32> {
    let reader = if filename == "-" {
        match Reader::from_input() {
            Ok(reader) => reader,

            Err(_) => {
                println!("unable to read from stdin.");
                return Err(1);
            }
        }
    } else {
        match Reader::from_file(filename) {
            Ok(reader) => reader,

            Err(_) => {
                println!("unable to read file `{}`", filename);
                return Err(1);
            }
        }
    };

    parse_reader(reader, vm, ast)
}

fn parse_reader(reader: Reader, vm: &mut VM, ast: &mut Ast) -> Result<(), i32> {
    let filename: String = reader.path().into();
    let parser = Parser::new(reader, &vm.id_generator, ast, &mut vm.interner);

    match parser.parse() {
        Ok(file) => {
            vm.files.push(file);
            assert_eq!(ast.files.len(), vm.files.len());
            Ok(())
        }

        Err(error) => {
            println!(
                "error in {} at {}: {}",
                filename,
                error.pos,
                error.error.message()
            );
            println!("error during parsing.");

            Err(1)
        }
    }
}

#[cfg(not(test))]
fn main() {
    exit(start());
}