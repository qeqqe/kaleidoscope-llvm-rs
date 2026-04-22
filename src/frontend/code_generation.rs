use std::collections::HashMap;

use inkwell::{
    FloatPredicate,
    builder::Builder,
    context::Context,
    module::Module,
    types::BasicMetadataTypeEnum,
    values::{BasicMetadataValueEnum, BasicValue, FloatValue, FunctionValue},
};

use crate::frontend::parser::{ExprAST, FunctionAST, PrototypeAST};

pub struct Compiler<'ctx> {
    pub context: &'ctx Context,
    pub builder: Builder<'ctx>,
    pub module: Module<'ctx>,

    /// maps parameter names to their SSA value pointers
    named_values: HashMap<String, FloatValue<'ctx>>,
}

pub type CodegenResult<T> = Result<T, String>;

impl<'ctx> Compiler<'ctx> {
    pub fn new(context: &'ctx Context, module_name: &str) -> Self {
        Self {
            context,
            builder: context.create_builder(),
            module: context.create_module(module_name),
            named_values: HashMap::new(),
        }
    }

    pub fn emit_expr(&mut self, expr: &ExprAST) -> CodegenResult<FloatValue<'ctx>> {
        match expr {
            ExprAST::Number(n) => self.emit_number(*n),
            ExprAST::Variable(v) => self.emit_variable(v),
            ExprAST::Binary(op, lhs, rhs) => self.emit_binary(*op, lhs, rhs),
            ExprAST::Call(id_name, args) => self.emit_call(id_name, args),
        }
    }

    fn emit_number(&mut self, n: f64) -> CodegenResult<FloatValue<'ctx>> {
        Ok(self.context.f64_type().const_float(n))
    }

    fn emit_variable(&mut self, v: &str) -> CodegenResult<FloatValue<'ctx>> {
        self.named_values
            .get(v)
            .copied()
            .ok_or_else(|| format!("unknown variable {v}"))
    }

    fn emit_binary(
        &mut self,
        op: char,
        lhs: &ExprAST,
        rhs: &ExprAST,
    ) -> CodegenResult<FloatValue<'ctx>> {
        let l = self.emit_expr(lhs)?;
        let r = self.emit_expr(rhs)?;

        match op {
            '+' => Ok(self
                .builder
                .build_float_add(l, r, "addtmp")
                .map_err(|e| e.to_string())?),

            '-' => Ok(self
                .builder
                .build_float_sub(l, r, "subtmp")
                .map_err(|e| e.to_string())?),

            '*' => Ok(self
                .builder
                .build_float_mul(l, r, "multmp")
                .map_err(|e| e.to_string())?),

            '<' => {
                // fcmp returns i1; convert to f64 (0.0 or 1.0) with uitofp
                let cmp = self
                    .builder
                    .build_float_compare(FloatPredicate::ULT, l, r, "cmptmp")
                    .map_err(|e| e.to_string())?;

                Ok(self
                    .builder
                    .build_unsigned_int_to_float(cmp, self.context.f64_type(), "booltmp")
                    .map_err(|e| e.to_string())?)
            }

            _ => Err(format!("Unknown binary operator: '{op}'")),
        }
    }

    fn emit_call(&mut self, callee: &str, args: &[ExprAST]) -> CodegenResult<FloatValue<'ctx>> {
        let callee_fn = self
            .module
            .get_function(callee)
            .ok_or_else(|| format!("Unknown function referenced: '{callee}'"))?;

        if callee_fn.count_params() as usize != args.len() {
            return Err(format!(
                "Argument count mismatch: '{}' expects {}, got {}",
                callee,
                callee_fn.count_params(),
                args.len(),
            ));
        }

        let compiled_args: Vec<BasicMetadataValueEnum> = args
            .iter()
            .map(|arg| self.emit_expr(arg).map(|v| v.as_basic_value_enum().into()))
            .collect::<Result<_, _>>()?;

        let call = self
            .builder
            .build_call(callee_fn, &compiled_args, "calltmp")
            .map_err(|e| e.to_string())?;

        match call.try_as_basic_value() {
            inkwell::values::ValueKind::Basic(basic_val) => Ok(basic_val.into_float_value()),
            _ => Err(format!("Call to '{}' did not return a value", callee)),
        }
    }

    pub fn emit_prototype(&self, proto: &PrototypeAST) -> CodegenResult<FunctionValue<'ctx>> {
        let f64_type = self.context.f64_type();

        let param_types: Vec<BasicMetadataTypeEnum> =
            proto.1.iter().map(|_| f64_type.into()).collect();

        let fn_type = f64_type.fn_type(&param_types, false);

        let function = match self.module.get_function(&proto.0) {
            Some(f) => f,
            None => self.module.add_function(&proto.0, fn_type, None),
        };

        for (param, name) in function.get_param_iter().zip(proto.1.iter()) {
            param.into_float_value().set_name(name);
        }

        Ok(function)
    }

    pub fn emit_function(&mut self, func: &FunctionAST) -> CodegenResult<FunctionValue<'ctx>> {
        let FunctionAST(proto, body) = func;

        let function = self.emit_prototype(proto)?;

        if !function.is_null() && function.count_basic_blocks() > 0 {
            return Err(format!("Function '{}' cannot be redefined.", proto.0));
        }

        let entry_bb = self.context.append_basic_block(function, "entry");
        self.builder.position_at_end(entry_bb);

        self.named_values.clear();
        for param in function.get_param_iter() {
            let float_val = param.into_float_value();
            let name = float_val
                .get_name()
                .to_str()
                .map_err(|e| e.to_string())?
                .to_string();
            self.named_values.insert(name, float_val);
        }

        match self.emit_expr(body) {
            Ok(ret_val) => {
                self.builder
                    .build_return(Some(&ret_val))
                    .map_err(|e| e.to_string())?;

                if function.verify(true) {
                    Ok(function)
                } else {
                    unsafe { function.delete() };
                    Err(format!("Function '{}' failed verification.", proto.0))
                }
            }
            Err(e) => {
                unsafe { function.delete() };
                Err(e)
            }
        }
    }
}
