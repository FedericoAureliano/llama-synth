use std::cell::RefCell;
use std::mem;
use bit_vec::BitVec;

use crate::parser::cst::*;
use crate::parser::error::{ParseError, ParseErrorAndPos};

use crate::parser::interner::*;

use crate::parser::lexer::position::{Position, Span};
use crate::parser::lexer::reader::Reader;
use crate::parser::lexer::token::*;
use crate::parser::lexer::*;

pub struct Parser<'a> {
    lexer: Lexer,
    token: Token,
    id_generator: &'a NodeIdGenerator,
    interner: &'a mut Interner,
    cst: &'a mut Cst,
    param_idx: u32,
    in_class_or_module: bool,
    parse_struct_lit: bool,
    lcst_end: Option<u32>,
}

type ExprResult = Result<Box<ExprSyntaxObject>, ParseErrorAndPos>;
type StmtResult = Result<Box<StmtSyntaxObject>, ParseErrorAndPos>;

impl<'a> Parser<'a> {
    pub fn new(
        reader: Reader,
        id_generator: &'a NodeIdGenerator,
        cst: &'a mut Cst,
        interner: &'a mut Interner,
    ) -> Parser<'a> {
        let token = Token::new(TokenKind::End, Position::new(1, 1), Span::invalid());
        let lexer = Lexer::new(reader);

        let parser = Parser {
            lexer,
            token,
            id_generator,
            interner,
            param_idx: 0,
            in_class_or_module: false,
            parse_struct_lit: true,
            cst,
            lcst_end: Some(0),
        };

        parser
    }

    fn generate_id(&mut self) -> NodeId {
        self.id_generator.next()
    }

    pub fn parse(mut self) -> Result<(), ParseErrorAndPos> {
        self.init()?;
        let mut modules = vec![];

        while !self.token.is_eof() {
            self.parse_top_level_element(&mut modules)?;
        }

        self.cst.modules = modules;

        Ok(())
    }

    fn init(&mut self) -> Result<(), ParseErrorAndPos> {
        self.advance_token()?;

        Ok(())
    }

    fn parse_top_level_element(
        &mut self,
        modules: &mut Vec<ModuleSyntaxObject>,
    ) -> Result<(), ParseErrorAndPos> {
        let modifiers = self.parse_annotations()?;

        match self.token.kind {
            TokenKind::Module => {
                self.ban_modifiers(&modifiers)?;
                let module = self.parse_module()?;
                modules.push(module);
            }

            _ => {
                let msg = ParseError::ExpectedTopLevelElement(self.token.name());
                return Err(ParseErrorAndPos::new(self.token.position, msg));
            }
        }

        Ok(())
    }

    fn parse_module(&mut self) -> Result<ModuleSyntaxObject, ParseErrorAndPos> {
        let pos = self.expect_token(TokenKind::Module)?.position;
        let ident = self.expect_ident()?;
        let mut module = ModuleSyntaxObject {
            id: self.generate_id(),
            name: ident.name,
            pos: pos,

            types: Vec::new(),

            inputs: Vec::new(),
            outputs: Vec::new(),
            variables: Vec::new(),
            constants: Vec::new(),
            
            macros: Vec::new(),            
            functions: Vec::new(),
            procedures: Vec::new(),
            
            theorems: Vec::new(),
            lemmas: Vec::new(),
            
            init: None,
            next: None,
            control: None,
        };

        self.in_class_or_module = true;
        self.parse_module_body(&mut module)?;
        self.in_class_or_module = false;

        Ok(module)
    }

    fn parse_module_body(&mut self, module: &mut ModuleSyntaxObject) -> Result<(), ParseErrorAndPos> {
        if !self.token.is(TokenKind::LBrace) {
            return Ok(());
        }

        self.advance_token()?;

        while !self.token.is(TokenKind::RBrace) {
            let modifiers = self.parse_annotations()?;

            match self.token.kind {
                TokenKind::Input => {
                    self.ban_modifiers(&modifiers)?;

                    let field = self.parse_field()?;
                    module.inputs.push(field);
                }

                TokenKind::Output => {
                    self.ban_modifiers(&modifiers)?;

                    let field = self.parse_field()?;
                    module.outputs.push(field);
                }

                TokenKind::Var => {
                    self.ban_modifiers(&modifiers)?;

                    let field = self.parse_field()?;
                    module.variables.push(field);
                }

                TokenKind::Const => {
                    self.ban_modifiers(&modifiers)?;
                    let xconst = self.parse_field()?;
                    module.constants.push(xconst);
                }
    
                TokenKind::Type => {
                    self.ban_modifiers(&modifiers)?;
                    self.expect_token(TokenKind::Type)?;

                    let ty = self.parse_type_decl()?;
                    module.types.push(ty);
                }


                TokenKind::Define => {
                    self.ban_modifiers(&modifiers)?;
                    let def = self.parse_define()?;
                    module.macros.push(def);
                }

                TokenKind::Function => {
                    let mods = &[
                        Modifier::Synthesis,
                    ];
                    self.restrict_modifiers(&modifiers, mods)?;

                    let fct = self.parse_function(&modifiers)?;
                    module.functions.push(fct);
                }

                TokenKind::Procedure => {
                    let mods = &[
                        Modifier::Inline,
                    ];
                    self.restrict_modifiers(&modifiers, mods)?;

                    let fct = self.parse_procedure(&modifiers)?;
                    module.procedures.push(fct);
                }

                TokenKind::Theorem => {
                    self.ban_modifiers(&modifiers)?;

                    let spec = self.parse_theorem()?;
                    module.theorems.push(spec);
                }

                TokenKind::Lemma => {
                    self.ban_modifiers(&modifiers)?;

                    let spec = self.parse_lemma()?;
                    module.theorems.push(spec);
                }

                TokenKind::Init => {
                    self.ban_modifiers(&modifiers)?;

                    let block = self.parse_init()?;
                    module.init = Some(Box::new(block));
                }

                TokenKind::Next => {
                    self.ban_modifiers(&modifiers)?;

                    let block = self.parse_next()?;
                    module.next = Some(Box::new(block));
                }

                TokenKind::Control => {
                    self.ban_modifiers(&modifiers)?;

                    let block = self.parse_control()?;
                    module.control = Some(Box::new(block));
                }

                _ => {
                    return Err(ParseErrorAndPos::new(
                        self.token.position,
                        ParseError::ExpectedModuleElement(self.token.name()),
                    ))
                },
            }
        }

        self.advance_token()?;
        Ok(())
    }

    fn parse_annotations(&mut self) -> Result<Modifiers, ParseErrorAndPos> {
        let mut modifiers = Modifiers::new();
        loop {
            if !self.token.is(TokenKind::At) {
                break;
            }
            self.advance_token()?;
            let ident = self.expect_ident()?;
            let modifier = match self.interner.str(ident.name).as_str() {
                "inline" => Modifier::Inline,
                "synthesis" => Modifier::Synthesis,
                _ => {
                    return Err(ParseErrorAndPos::new(
                        self.token.position,
                        ParseError::UnknownAnnotation(self.token.to_string()),
                    ));
                }
            };

            if modifiers.contains(modifier) {
                return Err(ParseErrorAndPos::new(
                    self.token.position,
                    ParseError::RedundantAnnotation(self.token.name()),
                ));
            }

            modifiers.add(modifier, self.token.position, self.token.span);
        }

        Ok(modifiers)
    }

    fn ban_modifiers(&mut self, modifiers: &Modifiers) -> Result<(), ParseErrorAndPos> {
        self.restrict_modifiers(modifiers, &[])
    }

    fn restrict_modifiers(
        &mut self,
        modifiers: &Modifiers,
        restrict: &[Modifier],
    ) -> Result<(), ParseErrorAndPos> {
        for modifier in modifiers.iter() {
            if !restrict.contains(&modifier.value) {
                return Err(ParseErrorAndPos::new(
                    modifier.pos,
                    ParseError::MisplacedAnnotation(modifier.value.name().into()),
                ));
            }
        }

        Ok(())
    }

    fn parse_field(&mut self) -> Result<FieldDeclarationSyntaxObject, ParseErrorAndPos> {
        let start = self.token.span.start();
        let pos = self.token.position;

        if self.token.is(TokenKind::Var) {
            self.expect_token(TokenKind::Var)?;
        } else if self.token.is(TokenKind::Input) {
            self.expect_token(TokenKind::Input)?;
        } else if self.token.is(TokenKind::Output) {
            self.expect_token(TokenKind::Output)?;
        } else if self.token.is(TokenKind::Const) {
            self.expect_token(TokenKind::Const)?;
        } else {
            return Err(ParseErrorAndPos::new(
                self.token.position,
                ParseError::ExpectedToken("input, output, var, or const".into(), self.token.name()),
            ));
        };

        let name = self.expect_ident()?;
        self.expect_token(TokenKind::Colon)?;
        let data_type = self.parse_type_ident()?;

        let expr = if self.token.is(TokenKind::Eq) {
            self.expect_token(TokenKind::Eq)?;
            Some(self.parse_expression()?)
        } else {
            None
        };

        self.expect_semicolon()?;
        let span = self.span_from(start);

        Ok(FieldDeclarationSyntaxObject {
            id: self.generate_id(),
            name: name.name,
            pos,
            span,
            data_type,
            expr,
        })
    }

    fn parse_theorem(&mut self) -> Result<PropertyDeclarationSyntaxObject, ParseErrorAndPos> {
        let start = self.token.span.start();
        let pos = self.token.position;
        self.expect_token(TokenKind::Theorem)?;
        let name = self.expect_ident()?;
        self.expect_token(TokenKind::Colon)?;
        let expr = self.parse_expression()?;

        self.expect_semicolon()?;
        let span = self.span_from(start);

        Ok(PropertyDeclarationSyntaxObject {
            id: self.generate_id(),
            name: name.name,
            pos,
            span,
            expr,
        })
    }

    fn parse_lemma(&mut self) -> Result<PropertyDeclarationSyntaxObject, ParseErrorAndPos> {
        let start = self.token.span.start();
        let pos = self.token.position;
        self.expect_token(TokenKind::Lemma)?;
        let name = self.expect_ident()?;
        self.expect_token(TokenKind::Colon)?;
        let expr = self.parse_expression()?;

        self.expect_semicolon()?;
        let span = self.span_from(start);

        Ok(PropertyDeclarationSyntaxObject {
            id: self.generate_id(),
            name: name.name,
            pos,
            span,
            expr,
        })
    }
    fn parse_procedure(&mut self, _modifiers: &Modifiers) -> Result<ProcedureDeclarationSyntaxObject, ParseErrorAndPos> {
        let start = self.token.span.start();
        let pos = self.expect_token(TokenKind::Procedure)?.position;
        let ident = self.expect_ident()?;
        let params = self.parse_procedure_params()?;

        let mut returns = Vec::new();
        let mut requires = Vec::new();
        let mut modifies = Vec::new();
        let mut ensures = Vec::new();

        while self.token.is(TokenKind::Returns) || self.token.is(TokenKind::Modifies) || self.token.is(TokenKind::Requires) || self.token.is(TokenKind::Ensures) {
            if self.token.is(TokenKind::Returns) {
                let mut tmp_returns = self.parse_procedure_returns()?;
                returns.append(&mut tmp_returns);
            }

            if self.token.is(TokenKind::Modifies) {
                let mut tmp_modifies = self.parse_procedure_modifies()?;
                modifies.append(&mut tmp_modifies);
            }

            if self.token.is(TokenKind::Requires) {
                let tmp_requires = self.parse_procedure_requires()?;
                requires.push(tmp_requires);
            }

            if self.token.is(TokenKind::Ensures) {
                let tmp_ensures = self.parse_procedure_ensures()?;
                ensures.push(tmp_ensures);
            }
        };
        
        let block = self.parse_procedure_block()?;
        let span = self.span_from(start);

        Ok(ProcedureDeclarationSyntaxObject {
            id: self.generate_id(),
            name: ident.name,
            pos,
            span,
            params,
            returns,
            modifies,
            requires,
            ensures,
            block,
        })
    }

    fn parse_function(&mut self, modifiers: &Modifiers) -> Result<FunctionDeclarationSyntaxObject, ParseErrorAndPos> {
        let start = self.token.span.start();
        let pos = self.expect_token(TokenKind::Function)?.position;
        let ident = self.expect_ident()?;
        let params = self.parse_procedure_params()?;
        let return_type = self.parse_function_type()?;
        self.expect_semicolon()?;
        let span = self.span_from(start);

        Ok(FunctionDeclarationSyntaxObject {
            id: self.generate_id(),
            name: ident.name,
            pos,
            span,
            to_synthesize: modifiers.contains(Modifier::Synthesis),
            params,
            return_type,
        })
    }

    fn parse_define(&mut self) -> Result<MacroDeclarationSyntaxObject, ParseErrorAndPos> {
        let start = self.token.span.start();
        let pos = self.expect_token(TokenKind::Define)?.position;
        let ident = self.expect_ident()?;
        let params = self.parse_procedure_params()?;
        let return_type = self.parse_function_type()?;
        self.expect_token(TokenKind::Eq)?;
        let expr = self.parse_expression()?;
        self.expect_semicolon()?;
        let span = self.span_from(start);

        Ok(MacroDeclarationSyntaxObject {
            id: self.generate_id(),
            name: ident.name,
            pos,
            span,
            params,
            return_type,
            expr,
        })
    }

    fn parse_init(&mut self) -> Result<TransitionSystemBlockSyntaxObject, ParseErrorAndPos> {
        let start = self.token.span.start();
        let pos = self.expect_token(TokenKind::Init)?.position;
        if let Some(block) = self.parse_procedure_block()? {
            let span = self.span_from(start);
    
            Ok(TransitionSystemBlockSyntaxObject {
                id: self.generate_id(),
                pos,
                span,
                block,
            })
        } else {
            Err(ParseErrorAndPos::new(
                    self.token.position,
                    ParseError::ExpectedBlock,
                ))
        }
    }

    fn parse_next(&mut self) -> Result<TransitionSystemBlockSyntaxObject, ParseErrorAndPos> {
        let start = self.token.span.start();
        let pos = self.expect_token(TokenKind::Next)?.position;
        if let Some(block) = self.parse_procedure_block()? {
            let span = self.span_from(start);
    
            Ok(TransitionSystemBlockSyntaxObject {
                id: self.generate_id(),
                pos,
                span,
                block,
            })
        } else {
            Err(ParseErrorAndPos::new(
                    self.token.position,
                    ParseError::ExpectedBlock,
                ))
        }
    }

    fn parse_control(&mut self) -> Result<TransitionSystemBlockSyntaxObject, ParseErrorAndPos> {
        let start = self.token.span.start();
        let pos = self.expect_token(TokenKind::Control)?.position;
        if let Some(block) = self.parse_procedure_block()? {
            let span = self.span_from(start);
    
            Ok(TransitionSystemBlockSyntaxObject {
                id: self.generate_id(),
                pos,
                span,
                block,
            })
        } else {
            Err(ParseErrorAndPos::new(
                    self.token.position,
                    ParseError::ExpectedBlock,
                ))
        }
    }

    fn parse_procedure_params(&mut self) -> Result<Vec<ParamSyntaxObject>, ParseErrorAndPos> {
        self.expect_token(TokenKind::LParen)?;
        self.param_idx = 0;

        let params = self.parse_comma_list(TokenKind::RParen, |p| {
            p.param_idx += 1;

            p.parse_procedure_param()
        })?;

        Ok(params)
    }

    fn parse_comma_list<F, R>(
        &mut self,
        stop: TokenKind,
        mut parse: F,
    ) -> Result<Vec<R>, ParseErrorAndPos>
    where
        F: FnMut(&mut Parser) -> Result<R, ParseErrorAndPos>,
    {
        let mut data = vec![];
        let mut comma = true;

        while !self.token.is(stop.clone()) && !self.token.is_eof() {
            if !comma {
                return Err(ParseErrorAndPos::new(
                    self.token.position,
                    ParseError::ExpectedToken(TokenKind::Comma.name().into(), self.token.name()),
                ));
            }

            let entry = parse(self)?;
            data.push(entry);

            comma = self.token.is(TokenKind::Comma);
            if comma {
                self.advance_token()?;
            }
        }

        self.expect_token(stop)?;

        Ok(data)
    }

    fn parse_procedure_param(&mut self) -> Result<ParamSyntaxObject, ParseErrorAndPos> {
        let start = self.token.span.start();
        let pos = self.token.position;

        let name = self.expect_ident()?;

        self.expect_token(TokenKind::Colon)?;
        let data_type = self.parse_type_ident()?;
        let span = self.span_from(start);

        Ok(ParamSyntaxObject {
            id: self.generate_id(),
            idx: self.param_idx - 1,
            name: name.name,
            pos,
            span,
            data_type,
        })
    }

    fn parse_function_type(&mut self) -> Result<Option<TypeIdentifierSyntaxObject>, ParseErrorAndPos> {
        if self.token.is(TokenKind::Colon) {
            self.advance_token()?;
            let ty = self.parse_type_ident()?;

            Ok(Some(ty))
        } else {
            Ok(None)
        }
    }

    fn parse_procedure_returns(&mut self) -> Result<Vec<ParamSyntaxObject>, ParseErrorAndPos> {
        if !self.token.is(TokenKind::Returns) {
            return Ok(Vec::new())
        }
        self.expect_token(TokenKind::Returns)?;
        self.expect_token(TokenKind::LParen)?;
        self.param_idx = 0;

        let params = self.parse_comma_list(TokenKind::RParen, |p| {
            p.param_idx += 1;

            p.parse_procedure_param()
        })?;

        Ok(params)
    }

    fn parse_procedure_modifies(&mut self) -> Result<Vec<NameSyntaxObject>, ParseErrorAndPos> {
        if !self.token.is(TokenKind::Modifies) {
            return Ok(Vec::new())
        }
        self.expect_token(TokenKind::Modifies)?;
        self.param_idx = 0;

        let names = self.parse_comma_list(TokenKind::Semicolon, |p| {
            p.param_idx += 1;

            p.expect_ident()
        })?;

        Ok(names)
    }

    fn parse_procedure_block(&mut self) -> Result<Option<Box<BlockSyntaxObject>>, ParseErrorAndPos> {
        if self.token.is(TokenKind::Semicolon) {
            self.advance_token()?;

            Ok(None)
        } else {
            let block = self.parse_block()?;

            if let ExprSyntaxObject::Block(block_type) = *block {
                Ok(Some(Box::new(block_type)))
            } else {
                unreachable!()
            }
        }
    }

    fn parse_type_ident(&mut self) -> Result<TypeIdentifierSyntaxObject, ParseErrorAndPos> {
        match self.token.kind {
                                        // This gives us arrays
            TokenKind::Identifier(_) | TokenKind::LBracket => {
                let pos = self.token.position;
                let start = self.token.span.start();

                let params = if self.token.is(TokenKind::LBracket) {
                    self.advance_token()?;
                    self.parse_comma_list(TokenKind::RBracket, |p| Ok(Box::new(p.parse_type_ident()?)))?
                } else {
                    Vec::new()
                };

                let ident = self.expect_ident()?;

                let span = self.span_from(start);
                Ok(TypeIdentifierSyntaxObject::create_basic(
                    ident.id,
                    pos,
                    span,
                    ident.name,
                    params,
                ))
            }

            TokenKind::LParen => {
                let start = self.token.span.start();
                let token = self.advance_token()?;
                let subtypes = self.parse_comma_list(TokenKind::RParen, |p| {
                    let ty = p.parse_type_ident()?;

                    Ok(Box::new(ty))
                })?;

                let span = self.span_from(start);
                Ok(TypeIdentifierSyntaxObject::create_tuple(
                    self.generate_id(),
                    token.position,
                    span,
                    subtypes,
                ))
            }

            _ => Err(ParseErrorAndPos::new(
                self.token.position,
                ParseError::ExpectedType(self.token.name()),
            )),
        }
    }

    fn parse_type_decl(&mut self) -> Result<TypeDeclarationSyntaxObject, ParseErrorAndPos> {

        let pos = self.token.position;
        let start = self.token.span.start();
        let name = self.expect_ident()?;

        self.expect_token(TokenKind::Eq)?;

        let rhs = match self.token.kind {
            TokenKind::Enum => {
                let pos = self.expect_token(TokenKind::Enum)?.position;
                let start = self.token.span.start();

                let variants = if self.token.is(TokenKind::LBrace) {
                    self.advance_token()?;
                    self.parse_comma_list(TokenKind::RBrace, |p| Ok(p.expect_ident()?))?
                } else {
                    Vec::new()
                };

                let nvariants = variants.iter().map(|v| v.name).collect();

                let span = self.span_from(start);
                self.expect_semicolon()?;

                return Ok(TypeDeclarationSyntaxObject::create_enum(
                    name.id,
                    pos,
                    span,
                    name.name,
                    nvariants,
                ))
            }
                                        // This gives us arrays
            TokenKind::Identifier(_) | TokenKind::LBracket => {
                let pos = self.token.position;
                let start = self.token.span.start();

                let params = if self.token.is(TokenKind::LBracket) {
                    self.advance_token()?;
                    self.parse_comma_list(TokenKind::RBracket, |p| Ok(Box::new(p.parse_type_ident()?)))?
                } else {
                    Vec::new()
                };

                let name = self.expect_ident()?;

                let span = self.span_from(start);
                Ok(TypeIdentifierSyntaxObject::create_basic(
                    name.id,
                    pos,
                    span,
                    name.name,
                    params,
                ))
            }

            TokenKind::LParen => {
                let start = self.token.span.start();
                let token = self.advance_token()?;
                let subtypes = self.parse_comma_list(TokenKind::RParen, |p| {
                    let ty = p.parse_type_ident()?;

                    Ok(Box::new(ty))
                })?;

                let span = self.span_from(start);
                Ok(TypeIdentifierSyntaxObject::create_tuple(
                    self.generate_id(),
                    token.position,
                    span,
                    subtypes,
                ))
            }

            _ => Err(ParseErrorAndPos::new(
                self.token.position,
                ParseError::ExpectedType(self.token.name()),
            )),
        }?;

        self.expect_semicolon()?;
        let span = self.span_from(start);

        let ty = TypeDeclarationSyntaxObject::create_alias(
            name.id,
            pos,
            span,
            name.name,
            Box::new(rhs),
        );

        Ok(ty)
    }

    fn parse_procedure_requires(&mut self) -> Result<PredicateStmtSyntaxObject, ParseErrorAndPos> {
        let start = self.token.span.start();
        let pos = self.expect_token(TokenKind::Requires)?.position;
        let expr = self.parse_expression()?;

        self.expect_semicolon()?;
        let span = self.span_from(start);

        Ok(PredicateStmtSyntaxObject {
            id : self.generate_id(),
            pos,
            span,
            expr,
        })
    }

    fn parse_procedure_ensures(&mut self) -> Result<PredicateStmtSyntaxObject, ParseErrorAndPos> {
        let start = self.token.span.start();
        let pos = self.expect_token(TokenKind::Ensures)?.position;
        let expr = self.parse_expression()?;

        self.expect_semicolon()?;
        let span = self.span_from(start);

        Ok(PredicateStmtSyntaxObject {
            id : self.generate_id(),
            pos,
            span,
            expr,
        })
    }

    fn parse_assert(&mut self) -> StmtResult {
        let start = self.token.span.start();
        let pos = self.expect_token(TokenKind::Assert)?.position;
        let expr = self.parse_expression()?;

        self.expect_semicolon()?;
        let span = self.span_from(start);

        Ok(Box::new(StmtSyntaxObject::create_assert(
            self.generate_id(),
            pos,
            span,
            expr,
        )))
    }

    fn parse_assume(&mut self) -> StmtResult {
        let start = self.token.span.start();
        let pos = self.expect_token(TokenKind::Assume)?.position;
        let expr = self.parse_expression()?;

        self.expect_semicolon()?;
        let span = self.span_from(start);

        Ok(Box::new(StmtSyntaxObject::create_assume(
            self.generate_id(),
            pos,
            span,
            expr,
        )))
    }

    fn parse_var(&mut self) -> StmtResult {
        let start = self.token.span.start();
        let reassignable = if self.token.is(TokenKind::Const) {
            false
        } else {
            true
        };
        let pos = self.advance_token()?.position;
        let ident = self.expect_ident()?;
        let data_type = self.parse_var_type()?;
        let expr = self.parse_var_assignment()?;

        self.expect_semicolon()?;
        let span = self.span_from(start);

        Ok(Box::new(StmtSyntaxObject::create_var(
            ident.id,
            pos,
            span,
            ident.name,
            reassignable,
            data_type,
            expr,
        )))
    }

    fn parse_havoc(&mut self) -> StmtResult {
        // TODO: multiple names to havoc?
        let start = self.token.span.start();
        let pos = self.advance_token()?.position;
        let ident = self.expect_ident()?;

        self.expect_semicolon()?;
        let span = self.span_from(start);

        Ok(Box::new(StmtSyntaxObject::create_havoc(
            ident.id,
            pos,
            span,
            ident.name,
        )))
    }

    fn parse_induction(&mut self) -> StmtResult {
        let start = self.token.span.start();
        let pos = self.advance_token()?.position;
        self.expect_token(TokenKind::LParen)?;

        let mut steps : u64 = 0;

        if let TokenKind::LitInt(value, base, suffix) = &self.token.kind {
            let filtered = value.chars().filter(|&ch| ch != '_').collect::<String>();
            let parsed = u64::from_str_radix(&filtered, base.num());

            match parsed {
                Ok(num) => {
                    steps = num;
                }
                _ => {
                    let bits = match suffix {
                        IntSuffix::Byte => "byte",
                        IntSuffix::Int => "int",
                        IntSuffix::Long => "long",
                    };
                    return Err(ParseErrorAndPos::new(
                        pos,
                        ParseError::NumberOverflow(bits.into()),
                    ));
                }
            }
        } else {
            self.expect_token(TokenKind::LitInt("Some integer".into(), IntBase::Dec, IntSuffix::Int))?;
        }

        self.advance_token()?;
        self.expect_token(TokenKind::RParen)?;
        self.expect_semicolon()?;
        let span = self.span_from(start);

        Ok(Box::new(StmtSyntaxObject::create_induction(
            self.generate_id(),
            pos,
            span,
            steps,
        )))
    }

    fn parse_simulate(&mut self) -> StmtResult {
        let start = self.token.span.start();
        let pos = self.advance_token()?.position;
        self.expect_token(TokenKind::LParen)?;

        let mut steps : u64 = 0;

        if let TokenKind::LitInt(value, base, suffix) = &self.token.kind {
            let filtered = value.chars().filter(|&ch| ch != '_').collect::<String>();
            let parsed = u64::from_str_radix(&filtered, base.num());

            match parsed {
                Ok(num) => {
                    steps = num;
                }
                _ => {
                    let bits = match suffix {
                        IntSuffix::Byte => "byte",
                        IntSuffix::Int => "int",
                        IntSuffix::Long => "long",
                    };
                    return Err(ParseErrorAndPos::new(
                        pos,
                        ParseError::NumberOverflow(bits.into()),
                    ));
                }
            }
        } else {
            self.expect_token(TokenKind::LitInt("Some integer".into(), IntBase::Dec, IntSuffix::Int))?;
        }

        self.advance_token()?;
        self.expect_token(TokenKind::RParen)?;
        self.expect_semicolon()?;
        let span = self.span_from(start);

        Ok(Box::new(StmtSyntaxObject::create_simulate(
            self.generate_id(),
            pos,
            span,
            steps,
        )))
    }

    fn parse_call(&mut self) -> StmtResult {
        let start = self.token.span.start();
        let pos = self.advance_token()?.position;
        let rets = if self.token.is(TokenKind::LParen) {
            self.advance_token()?;
            let tmp = self.parse_comma_list(TokenKind::RParen, |p| p.expect_ident())?;
            self.expect_token(TokenKind::Eq)?;
            tmp
        } else {
            Vec::new()
        };

        let func = self.expect_ident()?;
        self.expect_token(TokenKind::LParen)?;
        let args = self.parse_comma_list(TokenKind::RParen, |p| p.parse_expression())?;

        self.expect_semicolon()?;
        let span = self.span_from(start);

        Ok(Box::new(StmtSyntaxObject::create_call(
            func.id,
            pos,
            span,
            func.name,
            rets,
            args,
        )))
    }

    fn parse_var_type(&mut self) -> Result<Option<TypeIdentifierSyntaxObject>, ParseErrorAndPos> {
        if self.token.is(TokenKind::Colon) {
            self.advance_token()?;

            Ok(Some(self.parse_type_ident()?))
        } else {
            Ok(None)
        }
    }

    fn parse_var_assignment(&mut self) -> Result<Option<Box<ExprSyntaxObject>>, ParseErrorAndPos> {
        if self.token.is(TokenKind::Eq) {
            self.expect_token(TokenKind::Eq)?;
            let expr = self.parse_expression()?;

            Ok(Some(expr))
        } else {
            Ok(None)
        }
    }

    fn parse_block_stmt(&mut self) -> StmtResult {
        let block = self.parse_block()?;
        Ok(Box::new(StmtSyntaxObject::create_expr_stmt(
            self.generate_id(),
            block.pos(),
            block.span(),
            block,
        )))
    }

    fn parse_block(&mut self) -> ExprResult {
        let start = self.token.span.start();
        let pos = self.expect_token(TokenKind::LBrace)?.position;
        let mut stmts = vec![];

        while !self.token.is(TokenKind::RBrace) && !self.token.is_eof() {
            let stmt = self.parse_statement()?;
            stmts.push(stmt);
        }

        self.expect_token(TokenKind::RBrace)?;
        let span = self.span_from(start);

        Ok(Box::new(ExprSyntaxObject::create_block(
            self.generate_id(),
            pos,
            span,
            stmts,
        )))
    }

    fn parse_statement(&mut self) -> StmtResult {
        match self.token.kind {
            TokenKind::Induction => self.parse_induction(),
            TokenKind::Simulate => self.parse_simulate(),
            TokenKind::Assert => self.parse_assert(),
            TokenKind::Assume => self.parse_assume(),
            TokenKind::Havoc => self.parse_havoc(),
            TokenKind::Call => self.parse_call(),
            TokenKind::Input 
            | TokenKind::Output 
            | TokenKind::Var 
            | TokenKind::Const => self.parse_var(),
            TokenKind::While => self.parse_while(),
            TokenKind::Else => Err(ParseErrorAndPos::new(
                self.token.position,
                ParseError::MisplacedElse,
            )),
            TokenKind::If => {
                let expr = self.parse_if()?;
                Ok(Box::new(StmtSyntaxObject::create_expr_stmt(
                    self.generate_id(),
                    expr.pos(),
                    expr.span(),
                    expr,
                )))
            },
            _ => {
                let expr = self.parse_expression()?;
                self.expect_token(TokenKind::Semicolon)?;
                let span = self.span_from(expr.span().start());

                Ok(Box::new(StmtSyntaxObject::create_expr_stmt(
                    self.generate_id(),
                    expr.pos(),
                    span,
                    expr,
                )))
            }
        }
    }

    fn parse_if(&mut self) -> ExprResult {
        let start = self.token.span.start();
        let pos = self.expect_token(TokenKind::If)?.position;

        let cond = self.parse_expression_no_struct_lit()?;

        let then_block = self.parse_block()?;

        let else_block = if self.token.is(TokenKind::Else) {
            self.advance_token()?;

            if self.token.is(TokenKind::If) {
                Some(self.parse_if()?)
            } else {
                Some(self.parse_block()?)
            }
        } else {
            None
        };

        let span = self.span_from(start);

        Ok(Box::new(ExprSyntaxObject::create_if(
            self.generate_id(),
            pos,
            span,
            cond,
            then_block,
            else_block,
        )))
    }

    fn parse_while(&mut self) -> StmtResult {
        let start = self.token.span.start();
        let pos = self.expect_token(TokenKind::While)?.position;
        let expr = self.parse_expression_no_struct_lit()?;
        let block = self.parse_block_stmt()?;
        let span = self.span_from(start);

        Ok(Box::new(StmtSyntaxObject::create_while(
            self.generate_id(),
            pos,
            span,
            expr,
            block,
        )))
    }

    fn parse_expression(&mut self) -> ExprResult {
        self.parse_expression_struct_lit(true)
    }

    fn parse_expression_no_struct_lit(&mut self) -> ExprResult {
        self.parse_expression_struct_lit(false)
    }

    fn parse_expression_struct_lit(&mut self, struct_lit: bool) -> ExprResult {
        let old = self.parse_struct_lit;
        self.parse_struct_lit = struct_lit;

        let result = match self.token.kind {
            TokenKind::LBrace => self.parse_block(),
            TokenKind::If => self.parse_if(),
            _ => self.parse_binary(0),
        };

        self.parse_struct_lit = old;

        result
    }

    fn parse_binary(&mut self, precedence: u32) -> ExprResult {
        let start = self.token.span.start();
        let mut left = self.parse_unary()?;

        loop {
            let right_precedence = match self.token.kind {
                TokenKind::Or => 1,
                TokenKind::And => 2,
                TokenKind::Eq | TokenKind::AddEq => 3,
                TokenKind::EqEq
                | TokenKind::Ne
                | TokenKind::Lt
                | TokenKind::Le
                | TokenKind::Gt
                | TokenKind::Ge
                | TokenKind::Ult
                | TokenKind::Ule
                | TokenKind::Ugt
                | TokenKind::Uge => 4,
                TokenKind::BitOr | TokenKind::BitAnd | TokenKind::Caret => 5,
                TokenKind::LtLt | TokenKind::GtGt | TokenKind::GtGtGt => 6,
                TokenKind::Add | TokenKind::Sub => 7,
                TokenKind::Mul | TokenKind::Div | TokenKind::Mod => 8,
                TokenKind::Concat => 9,
                TokenKind::Colon => 10,
                _ => {
                    return Ok(left);
                }
            };

            if precedence >= right_precedence {
                return Ok(left);
            }

            let tok = self.advance_token()?;

            left = match tok.kind {
                _ => {
                    let right = self.parse_binary(right_precedence)?;
                    self.create_binary(tok, start, left, right)
                }
            };
        }
    }

    fn parse_unary(&mut self) -> ExprResult {
        match self.token.kind {
            TokenKind::Add | TokenKind::Sub | TokenKind::Not => {
                let start = self.token.span.start();
                let tok = self.advance_token()?;
                let op = match tok.kind {
                    TokenKind::Add => UnOp::Plus,
                    TokenKind::Sub => UnOp::Neg,
                    TokenKind::Not => UnOp::Not,
                    _ => unreachable!(),
                };

                let expr = self.parse_primary()?;
                let span = self.span_from(start);
                Ok(Box::new(ExprSyntaxObject::create_un(
                    self.generate_id(),
                    tok.position,
                    span,
                    op,
                    expr,
                )))
            }

            _ => self.parse_primary(),
        }
    }

    fn parse_primary(&mut self) -> ExprResult {
        let start = self.token.span.start();
        let mut left = self.parse_factor()?;

        loop {
            left = match self.token.kind {
                TokenKind::Dot => {
                    let tok = self.advance_token()?;
                    let rhs = self.parse_factor()?;
                    let span = self.span_from(start);

                    Box::new(ExprSyntaxObject::create_dot(
                        self.generate_id(),
                        tok.position,
                        span,
                        left,
                        rhs,
                    ))
                }

                TokenKind::LParen => {
                    let tok = self.advance_token()?;
                    let args =
                        self.parse_comma_list(TokenKind::RParen, |p| p.parse_expression())?;
                    let span = self.span_from(start);

                    Box::new(ExprSyntaxObject::create_call(
                        self.generate_id(),
                        tok.position,
                        span,
                        left,
                        args,
                    ))
                }

                TokenKind::LBracket => {
                    let tok = self.advance_token()?;
                    let args = self.parse_comma_list(TokenKind::RBracket, |p| p.parse_expression())?;
                    let span = self.span_from(start);

                    Box::new(ExprSyntaxObject::create_deref(
                        self.generate_id(),
                        tok.position,
                        span,
                        left,
                        args,
                    ))
                }

                _ => {
                    return Ok(left);
                }
            }
        }
    }

    fn create_binary(
        &mut self,
        tok: Token,
        start: u32,
        left: Box<ExprSyntaxObject>,
        right: Box<ExprSyntaxObject>,
    ) -> Box<ExprSyntaxObject> {
        let op = match tok.kind {
            TokenKind::Eq => BinOp::Assign,
            TokenKind::Or => BinOp::Or,
            TokenKind::And => BinOp::And,
            TokenKind::EqEq => BinOp::Cmp(CmpOp::Eq),
            TokenKind::Ne => BinOp::Cmp(CmpOp::Ne),
            TokenKind::Lt => BinOp::Cmp(CmpOp::Lt),
            TokenKind::Le => BinOp::Cmp(CmpOp::Le),
            TokenKind::Gt => BinOp::Cmp(CmpOp::Gt),
            TokenKind::Ge => BinOp::Cmp(CmpOp::Ge),
            TokenKind::Ult => BinOp::Cmp(CmpOp::Ult),
            TokenKind::Ule => BinOp::Cmp(CmpOp::Ule),
            TokenKind::Ugt => BinOp::Cmp(CmpOp::Ugt),
            TokenKind::Uge => BinOp::Cmp(CmpOp::Uge),
            TokenKind::BitOr => BinOp::BitOr,
            TokenKind::BitAnd => BinOp::BitAnd,
            TokenKind::Caret => BinOp::BitXor,
            TokenKind::Add => BinOp::Add,
            TokenKind::Concat => BinOp::Concat,
            TokenKind::Sub => BinOp::Sub,
            TokenKind::Mul => BinOp::Mul,
            TokenKind::Div => BinOp::Div,
            TokenKind::Mod => BinOp::Mod,
            TokenKind::LtLt => BinOp::ShiftL,
            TokenKind::GtGt => BinOp::ArithShiftR,
            TokenKind::GtGtGt => BinOp::LogicalShiftR,
            TokenKind::Colon => BinOp::Range,
            _ => panic!("unimplemented token {:?}", tok),
        };

        let span = self.span_from(start);

        Box::new(ExprSyntaxObject::create_bin(
            self.generate_id(),
            tok.position,
            span,
            op,
            left,
            right,
        ))
    }

    fn parse_factor(&mut self) -> ExprResult {
        match self.token.kind {
            TokenKind::LParen => self.parse_parentheses(),
            TokenKind::LBrace => self.parse_block(),
            TokenKind::If => self.parse_if(),
            TokenKind::LitInt(_, _, _) => self.parse_lit_int(),
            TokenKind::LitFloat(_, _) => self.parse_lit_float(),
            TokenKind::LitBitVec(_, _) => self.parse_lit_bitvec(),
            TokenKind::Identifier(_) => self.parse_ident(),
            TokenKind::True => self.parse_bool_literal(),
            TokenKind::False => self.parse_bool_literal(),
            _ => Err(ParseErrorAndPos::new(
                self.token.position,
                ParseError::ExpectedFactor(self.token.name().clone()),
            )),
        }
    }

    fn parse_ident(&mut self) -> ExprResult {
        let pos = self.token.position;
        let span = self.token.span;
        let name = self.expect_ident()?;

        Ok(Box::new(ExprSyntaxObject::create_ident(
            name.id,
            pos,
            span,
            name.name,
        )))
    }

    fn parse_parentheses(&mut self) -> ExprResult {
        let pos = self.token.position;
        let start = self.token.span.start();
        self.expect_token(TokenKind::LParen)?;
        let expr = self.parse_expression()?;

        if self.token.kind == TokenKind::Comma {
            let mut values = vec![expr];
            let span;

            loop {
                self.expect_token(TokenKind::Comma)?;

                if self.token.kind == TokenKind::RParen {
                    self.advance_token()?;
                    span = self.span_from(start);
                    break;
                }

                let expr = self.parse_expression()?;
                values.push(expr);

                if self.token.kind == TokenKind::RParen {
                    self.advance_token()?;
                    span = self.span_from(start);
                    break;
                }
            }

            Ok(Box::new(ExprSyntaxObject::create_tuple(
                self.generate_id(),
                pos,
                span,
                values,
            )))
        } else {
            self.expect_token(TokenKind::RParen)?;

            Ok(expr)
        }
    }

    fn parse_lit_int(&mut self) -> ExprResult {
        let span = self.token.span;
        let tok = self.advance_token()?;
        let pos = tok.position;

        if let TokenKind::LitInt(value, base, suffix) = tok.kind {
            let filtered = value.chars().filter(|&ch| ch != '_').collect::<String>();
            let parsed = u64::from_str_radix(&filtered, base.num());

            match parsed {
                Ok(num) => {
                    let expr =
                        ExprSyntaxObject::create_lit_int(self.generate_id(), pos, span, num, base, suffix);
                    Ok(Box::new(expr))
                }

                _ => {
                    let bits = match suffix {
                        IntSuffix::Byte => "byte",
                        IntSuffix::Int => "int",
                        IntSuffix::Long => "long",
                    };

                    Err(ParseErrorAndPos::new(
                        pos,
                        ParseError::NumberOverflow(bits.into()),
                    ))
                }
            }
        } else {
            unreachable!();
        }
    }

    fn parse_lit_float(&mut self) -> ExprResult {
        let span = self.token.span;
        let tok = self.advance_token()?;
        let pos = tok.position;

        if let TokenKind::LitFloat(value, suffix) = tok.kind {
            let filtered = value.chars().filter(|&ch| ch != '_').collect::<String>();
            let parsed = filtered.parse::<f64>();

            if let Ok(num) = parsed {
                let expr = ExprSyntaxObject::create_lit_float(self.generate_id(), pos, span, num, suffix);
                return Ok(Box::new(expr));
            }
        }

        unreachable!()
    }

    fn parse_lit_bitvec(&mut self) -> ExprResult {
        let span = self.token.span;
        let tok = self.advance_token()?;
        let pos = tok.position;

        if let TokenKind::LitBitVec(value, width) = tok.kind {
            let filtered_value = value.chars().filter(|&ch| ch != '_').collect::<String>();
            let parsed_value = filtered_value.parse::<usize>();

            let filtered_width = width.chars().filter(|&ch| ch != '_').collect::<String>();
            let parsed_width = filtered_width.parse::<usize>();

            if let Ok(mut v) = parsed_value {
                if let Ok(w) = parsed_width {
                    let bv = BitVec::from_fn(w, |_| { 
                        let res = v % 2 == 1;
                        v = v / 2;
                        res 
                    });

                    let expr = ExprSyntaxObject::create_lit_bitvec(self.generate_id(), pos, span, bv);
                    return Ok(Box::new(expr));
                }
            }
        }

        unreachable!()
    }

    fn parse_bool_literal(&mut self) -> ExprResult {
        let span = self.token.span;
        let tok = self.advance_token()?;
        let value = tok.is(TokenKind::True);

        Ok(Box::new(ExprSyntaxObject::create_lit_bool(
            self.generate_id(),
            tok.position,
            span,
            value,
        )))
    }

    fn expect_ident(&mut self) -> Result<NameSyntaxObject, ParseErrorAndPos> {
        let span = self.token.span;
        let pos = self.token.position;
        let tok = self.advance_token()?;

        if let TokenKind::Identifier(ref value) = tok.kind {
            let interned = self.interner.intern(value);
            let name = NameSyntaxObject {
                id: self.generate_id(),
                pos,
                span,
                name: interned,
            };
            Ok(name)
        } else {
            Err(ParseErrorAndPos::new(
                tok.position,
                ParseError::ExpectedIdentifier(tok.name()),
            ))
        }
    }

    fn expect_semicolon(&mut self) -> Result<Token, ParseErrorAndPos> {
        self.expect_token(TokenKind::Semicolon)
    }

    fn expect_token(&mut self, kind: TokenKind) -> Result<Token, ParseErrorAndPos> {
        if self.token.kind == kind {
            let token = self.advance_token()?;

            Ok(token)
        } else {
            Err(ParseErrorAndPos::new(
                self.token.position,
                ParseError::ExpectedToken(kind.name().into(), self.token.name()),
            ))
        }
    }

    fn advance_token(&mut self) -> Result<Token, ParseErrorAndPos> {
        let token = self.lexer.read_token()?;
        Ok(self.advance_token_with(token))
    }

    fn advance_token_with(&mut self, token: Token) -> Token {
        self.lcst_end = if self.token.span.is_valid() {
            Some(self.token.span.end())
        } else {
            None
        };

        mem::replace(&mut self.token, token)
    }

    fn span_from(&self, start: u32) -> Span {
        Span::new(start, self.lcst_end.unwrap() - start)
    }
}

#[derive(Debug)]
pub struct NodeIdGenerator {
    value: RefCell<usize>,
}

impl NodeIdGenerator {
    pub fn new() -> NodeIdGenerator {
        NodeIdGenerator {
            value: RefCell::new(1),
        }
    }

    pub fn next(&self) -> NodeId {
        let value = *self.value.borrow();
        *self.value.borrow_mut() += 1;

        NodeId(value)
    }
}
