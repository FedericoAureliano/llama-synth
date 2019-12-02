use pest::Parser;
use pest::error::Error;
use pest::iterators::Pair;

use crate::context::{Context, Solution};
use crate::term::{Term, apply};

#[derive(Parser)]
#[grammar = "pest/synth.pest"]
struct SynthParser;

impl Context {
    fn parse_fapp(&self, pair: Pair<Rule>) -> Result<Term, Error<Rule>> {
        match pair.as_rule() {
            Rule::fapp => {
                let mut inner = pair.into_inner();
                let func = inner.next().unwrap().as_span().as_str();
                let mut args : Vec<Term> = vec! [];
                for i in inner {
                    args.push(self.parse_fapp(i)?)
                }
                Ok(apply(func, args))
            },
            _ => Err(Error::new_from_span(pest::error::ErrorVariant::CustomError{
                        message: "expecting function application!".to_owned(),
                    }, pair.as_span())),
        }
    }

    fn parse_param(&self, pair: Pair<Rule>) -> Result<((String, String)), Error<Rule>> {
        match pair.as_rule() {
            Rule::param => {
                let mut inner = pair.into_inner();
                let name = inner.next().unwrap().as_span().as_str().to_owned();
                let sort = inner.next().unwrap().as_span().as_str().to_owned();
                Ok((name, sort))
            },
            _ => Err(Error::new_from_span(pest::error::ErrorVariant::CustomError{
                        message: "expecting function application!".to_owned(),
                    }, pair.as_span())),
        }
    }

    fn parse_command(&mut self, pair: Pair<Rule>) -> Result<(), Error<Rule>> {
        match pair.as_rule() {
            Rule::setlogic => {
                let mut inner = pair.into_inner();
                let name = inner.next().unwrap().as_span().as_str().to_owned();
                self.set_logic(name);
                Ok(())
            }
            Rule::declare => { 
                let mut inner = pair.into_inner();
                let name = inner.next().unwrap().as_span().as_str().to_owned();

                let mut sorts = vec! []; 
                for s in inner {
                    sorts.push(s.as_span().as_str().to_owned());
                }

                let rsort = sorts.pop().unwrap();
                self.declare_fun(&name, sorts, rsort);
                Ok(())
            }
            Rule::define => { 
                let mut inner = pair.into_inner();
                let name = inner.next().unwrap().as_span().as_str().to_owned();

                let mut defn = vec! []; 
                for s in inner {
                    defn.push(s);
                }

                let body = self.parse_fapp(defn.pop().unwrap())?;
                let rsort = defn.pop().unwrap().as_span().as_str().to_owned();
                let params = defn.into_iter().map(|r| self.parse_param(r).expect("something wrong with parameter pair")).collect();
                self.define_fun(&name, params, rsort, body);
                Ok(())
            }
            Rule::checksat => {self.check_sat(); Ok(())},
            Rule::getmodel => {self.get_model(); Ok(())},
            Rule::assert => {
                let node = self.parse_fapp(pair.into_inner().next().unwrap())?;
                self.assert(node);
                Ok(())
            },
            Rule::push => {self.push(); Ok(())},
            Rule::pop => {self.pop(); Ok(())},
            _ => Err(Error::new_from_span(pest::error::ErrorVariant::CustomError{
                        message: "command not supported!".to_owned(),
                    }, pair.as_span())),
        }
    }

    fn parse_model(&self, pair: Pair<Rule>) -> Result<(String, (Vec<(String, String)>, String, Term)), Error<Rule>> {
        match pair.as_rule() {
            Rule::define => { 
                let mut inner = pair.into_inner();
                let name = inner.next().unwrap().as_span().as_str().to_owned();

                let mut defn = vec! []; 
                for s in inner {
                    defn.push(s);
                }

                let body = self.parse_fapp(defn.pop().unwrap())?;
                let rsort = defn.pop().unwrap().as_span().as_str().to_owned();
                let params = defn.into_iter().map(|r| self.parse_param(r).expect("something wrong with parameter pair")).collect();
                Ok((name, (params, rsort, body)))
            }
            _ => Err(Error::new_from_span(pest::error::ErrorVariant::CustomError{
                        message: "command not supported!".to_owned(),
                    }, pair.as_span())),
        }
    }

    pub fn parse_query(&mut self, file: &str) -> Result<(), Error<Rule>> {
        let syntax = SynthParser::parse(Rule::query, file).expect("failed to read!");    
        let mut empty = false;
        for r in syntax {
            self.parse_command(r)?;
            empty = true
        };
        assert!(empty, "problem with grammar: query is empty!");
        Ok(())
    }

    pub fn parse_answer(&self, file: &str) -> Result<Solution, Error<Rule>> {
        let syntax = SynthParser::parse(Rule::query, file).expect("failed to read!");    
        let mut sol = Solution::new();
        for r in syntax {
            let (name, entry) = self.parse_model(r)?;
            sol.insert(name, entry);
        };
        Ok(sol)
    }
}

#[cfg(test)]
mod test {
    use super::Context;

    #[test]
    fn test_parse_query() {
        use std::fs;
        let unparsed_file = fs::read_to_string("examples/qfuflia.smt2").expect("cannot read file");
        let mut q = Context::new();
        q.parse_query(&unparsed_file).unwrap();
        assert_eq!(unparsed_file, format!("{}", q));
    }

    #[test]
    fn test_parse_answer() {
        use std::fs;
        let unparsed_file = fs::read_to_string("examples/qfuflia_result.smt2").expect("cannot read file");
        let q = Context::new();
        let sol = q.parse_answer(&unparsed_file).unwrap();
        let (_, _, x_term) = sol.get("x").expect("couldn't find x");
        assert_eq!("8", format!("{}", x_term));
        let (_, _, f_term) = sol.get("f").expect("couldn't find f");
        assert_eq!("(- 1)", format!("{}", f_term));
    }
}