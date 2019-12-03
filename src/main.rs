use std::fs;
use std::env;

#[macro_use]
extern crate pest_derive;
extern crate pest;

#[macro_use] 
extern crate log;
extern crate env_logger;

#[macro_use]
extern crate enum_display_derive;

extern crate multimap;

mod context;
mod eval;
mod term;
mod query;

fn main() {
    env_logger::init();

    let args: Vec<String> = env::args().collect();

    let f = args.get(1).expect("no query file given!");
    let g = args.get(2).expect("no result file given!");

    let unparsed_query = fs::read_to_string(f).expect("cannot read file");
    let mut query = query::Query::new();
    query.parse_query(&unparsed_query).expect("cannot parse file");
    
    let unparsed_answer = fs::read_to_string(g).expect("cannot read file");
    let sol = query.parse_answer(&unparsed_answer).expect("cannot parse file");
    println!("{}", query);
    println!("evaluates to {}", query.eval(&sol));
}