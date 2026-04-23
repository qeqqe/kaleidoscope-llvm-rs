use std::collections::HashMap;

use inkwell::{
    FloatPredicate, OptimizationLevel,
    builder::Builder,
    context::Context,
    execution_engine::ExecutionEngine,
    module::Module,
    passes::PassBuilderOptions,
    targets::{CodeModel, InitializationConfig, RelocMode, Target, TargetMachine},
    types::BasicMetadataTypeEnum,
    values::{BasicMetadataValueEnum, BasicValue, FloatValue, FunctionValue},
};

use crate::frontend::parser::{ExprAST, FunctionAST, PrototypeAST};

pub struct Compiler<'ctx> {
    pub context: &'ctx Context,
    pub builder: Builder<'ctx>,
    pub module: Module<'ctx>,
    pub execution_engine: ExecutionEngine<'ctx>,

    /// maps parameter names to their SSA value pointers
    named_values: HashMap<String, FloatValue<'ctx>>,
}

pub type CodegenResult<T> = Result<T, String>;

fn create_target_machine() -> TargetMachine {
    Target::initialize_native(&InitializationConfig::default())
        .expect("Failed to initialize native target");

    let triple = TargetMachine::get_default_triple();
    let cpu = TargetMachine::get_host_cpu_name().to_string();
    let features = TargetMachine::get_host_cpu_features().to_string();

    Target::from_triple(&triple)
        .unwrap()
        .create_target_machine(
            &triple,
            &cpu,
            &features,
            OptimizationLevel::None,
            RelocMode::Default,
            CodeModel::Default,
        )
        .unwrap()
}

pub fn optimize_function(module: &Module, machine: &TargetMachine) -> CodegenResult<()> {
    let opts = PassBuilderOptions::create();

    module
        .run_passes(
            "function(instcombine,reassociate,gvn,simplifycfg,mem2reg)",
            machine,
            opts,
        )
        .map_err(|e| e.to_string())
}

impl<'ctx> Compiler<'ctx> {
    pub fn new(context: &'ctx Context, module_name: &str) -> Self {
        let module = context.create_module(module_name);
        let execution_engine = module
            .create_jit_execution_engine(OptimizationLevel::Aggressive)
            .unwrap();
        let target_data = execution_engine.get_target_data();
        let data_layout = target_data.get_data_layout();
        module.set_data_layout(&data_layout);
        module.set_triple(&TargetMachine::get_default_triple());

        let opts = PassBuilderOptions::create();
        opts.set_loop_unrolling(true);
        opts.set_loop_interleaving(true);
        opts.set_loop_vectorization(true);
        opts.set_loop_slp_vectorization(true);
        opts.set_merge_functions(false);
        opts.set_verify_each(true);

        let _ = module.create_jit_execution_engine(OptimizationLevel::Aggressive);

        let machine = &create_target_machine();

        let passes = "default<O3>";

        module.run_passes(passes, machine, opts).unwrap();

        Self {
            context,
            builder: context.create_builder(),
            module,
            execution_engine,
            named_values: HashMap::new(),
        }
    }

    pub fn run_anon_expr(&self) -> Result<f64, String> {
        type AnonFn = unsafe extern "C" fn() -> f64;
        unsafe {
            let f = self
                .execution_engine
                .get_function::<AnonFn>("__anon_expr")
                .map_err(|e| format!("jit lookup failed: {e}"))?;
            Ok(f.call())
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
                    optimize_function(&self.module, &create_target_machine())?;
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

    pub fn handle_top_level_expr(&mut self) -> CodegenResult<()> {
        todo!()
    }
}
