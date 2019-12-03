pub mod logic;
pub mod sort;

use std::collections::{HashMap};
use multimap::MultiMap;

use crate::term::Term;
use crate::context::sort::Sort;
use crate::context::logic::Logic;


pub type Sig = (Vec<(String, Sort)>, Sort);

pub struct Context {
    symbol_tbl: MultiMap<String, Sig>,
    body_tbl: HashMap<String, Term>,
    logic: Logic,
}

impl Context {
    pub fn new() -> Context {
        let mut ctx = Context {
            symbol_tbl: MultiMap::new(),
            body_tbl: HashMap::new(),
            logic: Logic::new(),
        };
        ctx.add_booleans();
        ctx
    }

    pub fn get_decl(&self, name: &str) -> Option<&Vec<Sig>> {
        // some interpreted functions are polymorphic (e.g. =)
        self.symbol_tbl.get_vec(name)
    }

    pub fn add_decl(&mut self, name: &str, params: Vec<(String, Sort)>, rsort: Sort) {
        // can declare each function exactly once
        assert!(!self.symbol_tbl.contains_key(name));
        assert!(self.logic.uf || params.len() == 0);
        self.symbol_tbl.insert(name.to_owned(), (params, rsort));
    }
    
    pub fn get_body(&self, name: &str) -> Option<&Term> {
        self.body_tbl.get(name)
    }
    
    pub fn add_body(&mut self, name: &str, body: Term) {
        // the new body has to be associated to exactly one function
        assert_eq!(self.symbol_tbl.get_vec(name).unwrap().len(), 1);
        self.body_tbl.insert(name.to_owned(), body);
    }

    pub fn set_logic(&mut self, l: &Logic) {
        if l.q {
            panic!("quantifiers not supported yet");
        }
        if l.lia {
            self.logic.lia = true;
            self.add_integers();
        }
        if l.uf {
            self.logic.uf = true;
        }
    }

    pub fn get_logic(&self) -> &Logic {
        &self.logic
    }

    pub fn get_sort(&self, t: &Term) -> Option<&Sort> {
    let arg_sorts: Vec<&Sort> = t.peek_args()
        .into_iter()
        .inspect(|x| debug!("checking {}", x))
        .map(|a| self.get_sort(a)
        .expect("term not well-formed!"))
        .collect();

        match self.get_decl(t.peek_name()) {
            Some(v) => {
                for (params, rsort) in v {
                    let exp_sorts: Vec<&Sort> = params.into_iter().map(|(_, s)| s).collect();
                    if exp_sorts.len() != arg_sorts.len() {
                        continue
                    };
                    let mut result = true;
                    for i in 0..arg_sorts.len() {
                        result = result && exp_sorts[i] == arg_sorts[i];
                    }
                    if result {
                        debug!("name: {} rsort: {}", t.peek_name(), rsort);
                        return Some(rsort)
                    }  
                }
                None
            }
            None => match t.peek_name().parse::<i64>() {
                Ok(_) => {
                    debug!("symbol: {} Int", t.peek_name()); 
                    Some(&Sort::Int)
                }
                Err(_) => None
            }
        }

    }

    fn add_booleans(&mut self) {
        // only support upto 4-ary
        for op in vec! ["and", "or", "="] {
            for names in vec! [vec! ["a", "b"], vec! ["a", "b", "c"], vec! ["a", "b", "c", "d"]] {
                let params = names.into_iter().map(|n| (n.to_owned(), Sort::Bool)).collect();
                self.symbol_tbl.insert(op.to_owned(), (params, Sort::Bool));
            }
        }
        self.symbol_tbl.insert("not".to_owned(), (vec![("a".to_owned(), Sort::Bool)], Sort::Bool));
        self.symbol_tbl.insert("=>".to_owned(), (vec![("a".to_owned(), Sort::Bool), ("b".to_owned(), Sort::Bool)], Sort::Bool));
        self.symbol_tbl.insert("ite".to_owned(), (vec![("a".to_owned(), Sort::Bool), ("b".to_owned(), Sort::Bool), ("c".to_owned(), Sort::Bool)], Sort::Bool));
    }

    fn add_integers(&mut self) {
        // only support upto 4-ary
        for op in vec! ["="] {
            for names in vec! [vec! ["a", "b"], vec! ["a", "b", "c"], vec! ["a", "b", "c", "d"]] {
                let params = names.into_iter().map(|n| (n.to_owned(), Sort::Int)).collect();
                self.symbol_tbl.insert(op.to_owned(), (params, Sort::Bool));
            }
        }
        // binary
        for op in vec! ["<", "<=", ">", ">="] {
            for names in vec! [vec! ["a", "b"]] {
                let params = names.into_iter().map(|n| (n.to_owned(), Sort::Int)).collect();
                self.symbol_tbl.insert(op.to_owned(), (params, Sort::Bool));
            }
        }
        // only support upto 4-ary
        for op in vec! ["+", "*", "-"] {
            for names in vec! [vec! ["a", "b"], vec! ["a", "b", "c"], vec! ["a", "b", "c", "d"]] {
                let params = names.into_iter().map(|n| (n.to_owned(), Sort::Int)).collect();
                self.symbol_tbl.insert(op.to_owned(), (params, Sort::Int));
            }
        }
        self.symbol_tbl.insert("-".to_owned(), (vec![("a".to_owned(), Sort::Int)], Sort::Int));
        self.symbol_tbl.insert("ite".to_owned(), (vec![("a".to_owned(), Sort::Bool), ("b".to_owned(), Sort::Int), ("c".to_owned(), Sort::Int)], Sort::Int));
    }
}