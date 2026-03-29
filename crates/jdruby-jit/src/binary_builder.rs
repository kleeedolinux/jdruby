//! # Binary Builder — Single Binary with Embedded Runtime
//!
//! Creates a standalone executable that bundles:
//! - The compiled Ruby code (as LLVM IR or native code)
//! - The JDRuby runtime (jdruby-runtime, jdruby-ffi)
//! - The JDGC garbage collector
//!
//! This produces a single binary that can run without external dependencies.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use inkwell::context::Context;
use inkwell::module::Module;
use inkwell::targets::{CodeModel, FileType, InitializationConfig, RelocMode, Target, TargetMachine, TargetTriple};
use inkwell::OptimizationLevel;
use jdruby_codegen::{CodeGenerator, CodegenConfig, OutputFormat};
use jdruby_common::JDRubyError;
use jdruby_mir::MirModule;

/// Configuration for building a standalone binary.
#[derive(Debug, Clone)]
pub struct BinaryBuilderConfig {
    /// Output path for the binary.
    pub output_path: PathBuf,
    /// Target triple (e.g., "x86_64-unknown-linux-gnu").
    pub target_triple: String,
    /// Optimization level.
    pub opt_level: OptimizationLevel,
    /// Whether to strip debug symbols.
    pub strip_symbols: bool,
    /// Static link the runtime (default: true for single binary).
    pub static_link: bool,
}

impl Default for BinaryBuilderConfig {
    fn default() -> Self {
        Self {
            output_path: PathBuf::from("a.out"),
            target_triple: "x86_64-unknown-linux-gnu".to_string(),
            opt_level: OptimizationLevel::Aggressive,
            strip_symbols: false,
            static_link: true,
        }
    }
}

/// Builder for creating standalone JDRuby binaries.
pub struct BinaryBuilder<'ctx> {
    context: &'ctx Context,
    config: BinaryBuilderConfig,
    modules: HashMap<String, Module<'ctx>>,
}

impl<'ctx> BinaryBuilder<'ctx> {
    /// Create a new binary builder.
    pub fn new(context: &'ctx Context, config: BinaryBuilderConfig) -> Self {
        Target::initialize_native(&InitializationConfig::default())
            .expect("Failed to initialize native target");

        Self {
            context,
            config,
            modules: HashMap::new(),
        }
    }

    /// Add a MIR module to be compiled into the binary with detailed error reporting.
    pub fn add_module_with_errors(&mut self, name: &str, mir: &MirModule) -> Result<(), JDRubyError> {
        let codegen_config = CodegenConfig {
            target_triple: self.config.target_triple.clone(),
            output_format: OutputFormat::LlvmIr,
            ..Default::default()
        };

        let mut codegen = CodeGenerator::new(codegen_config, self.context);
        
        // Use generate_module which returns Module directly, avoiding text IR parsing
        let module = codegen.generate_module(mir)
            .map_err(|diagnostics| {
                let messages: Vec<String> = diagnostics.iter()
                    .map(|d| d.message.clone())
                    .collect();
                JDRubyError::Codegen { 
                    message: format!("Code generation failed for module {}: {}", name, messages.join(", "))
                }
            })?;

        self.modules.insert(name.to_string(), module);
        Ok(())
    }

    /// Add a MIR module (legacy API, now uses new error system internally).
    pub fn add_module(&mut self, name: &str, mir: &MirModule) -> Result<(), String> {
        self.add_module_with_errors(name, mir)
            .map_err(|e| e.to_string())
    }

    /// Get the target machine for the configured target.
    fn get_target_machine(&self) -> Result<TargetMachine, String> {
        let target = Target::from_name(&self.config.target_triple)
            .or_else(|| {
                Target::from_triple(&TargetTriple::create(&self.config.target_triple)).ok()
            })
            .ok_or_else(|| format!("Unknown target: {}", self.config.target_triple))?;

        let target_machine = target
            .create_target_machine(
                &TargetTriple::create(&self.config.target_triple),
                "generic",
                "",
                self.config.opt_level,
                if self.config.static_link {
                    RelocMode::Static
                } else {
                    RelocMode::Default
                },
                CodeModel::Default,
            )
            .ok_or("Failed to create target machine")?;

        Ok(target_machine)
    }

    /// Link all modules into a single module.
    fn link_modules(&self) -> Result<Module<'ctx>, String> {
        let main_module = self.context.create_module("jdruby_main");
        
        eprintln!("DEBUG: Linking {} modules", self.modules.len());

        for (name, module) in &self.modules {
            main_module
                .link_in_module(module.clone())
                .map_err(|e| format!("Failed to link module {}: {}", name, e.to_string()))?;
        }
        
        // Re-emit runtime declarations after linking
        jdruby_codegen::runtime::emit_runtime_decls(self.context, &main_module);
        
        eprintln!("DEBUG: Linked module created successfully");

        Ok(main_module)
    }

    /// Build the final binary.
    pub fn build(&self) -> Result<PathBuf, String> {
        let target_machine = self.get_target_machine()?;
        let main_module = self.link_modules()?;

        // Write object file (optimization handled by TargetMachine opt_level)
        let obj_path = self.config.output_path.with_extension("o");
        target_machine
            .write_to_file(&main_module, FileType::Object, &obj_path)
            .map_err(|e| format!("Failed to write object file: {}", e.to_string()))?;

        // Link with runtime to create final binary
        self.link_runtime(&obj_path)?;

        // Clean up object file if requested
        if self.config.strip_symbols {
            let _ = std::fs::remove_file(&obj_path);
        }

        Ok(self.config.output_path.clone())
    }

    /// Link the object file with the JDRuby runtime.
    fn link_runtime(&self, obj_path: &Path) -> Result<(), String> {
        let output = std::process::Command::new("cc")
            .arg(obj_path)
            .arg("-o")
            .arg(&self.config.output_path)
            .args(self.get_linker_args())
            .output()
            .map_err(|e| format!("Linking failed: {}", e))?;

        if !output.status.success() {
            return Err(format!(
                "Linking failed: {}",
                String::from_utf8_lossy(&output.stderr)
            ));
        }

        Ok(())
    }

    /// Get linker arguments for linking with the runtime.
    fn get_linker_args(&self) -> Vec<String> {
        let mut args = vec![];

        // Add library search paths for target directory
        // Static libraries are built by cargo as lib<crate>.a files
        args.push("-L".to_string());
        args.push("target/debug".to_string());
        args.push("-L".to_string());
        args.push("target/release".to_string());

        // Note: removed -static flag as it prevents C runtime initialization
        // needed for I/O functions like println! to work properly
        // Add -no-pie since LLVM generates non-PIE object files
        args.push("-no-pie".to_string());

        // Export all dynamic symbols so dlsym can find compiled functions
        args.push("-rdynamic".to_string());
        args.push("-Wl,--export-dynamic".to_string());

        // Force static linking of Rust libraries
        args.push("-Wl,-Bstatic".to_string());

        // Link against JDRuby runtime libraries
        // Order matters: put most dependent libraries first
        args.push("-ljdruby_ffi".to_string());
        args.push("-ljdruby_runtime".to_string());
        args.push("-ljdgc".to_string());

        // Switch back to dynamic linking for system libraries
        args.push("-Wl,-Bdynamic".to_string());

        // System libraries - must come after Rust libraries
        // as Rust code depends on them
        args.push("-lpthread".to_string());
        args.push("-ldl".to_string());
        args.push("-lm".to_string());
        // Include C runtime for proper initialization
        args.push("-lc".to_string());

        if self.config.strip_symbols {
            args.push("-s".to_string());
        }

        args
    }

    /// Emit LLVM IR text instead of binary (for debugging).
    pub fn emit_llvm_ir(&self, output_path: &Path) -> Result<(), String> {
        let main_module = self.link_modules()?;
        main_module
            .print_to_file(output_path)
            .map_err(|e| format!("Failed to write LLVM IR: {}", e.to_string()))?;
        Ok(())
    }

    /// Emit assembly instead of binary (for debugging).
    pub fn emit_assembly(&self, output_path: &Path) -> Result<(), String> {
        let target_machine = self.get_target_machine()?;
        let main_module = self.link_modules()?;

        target_machine
            .write_to_file(&main_module, FileType::Assembly, output_path)
            .map_err(|e| format!("Failed to write assembly: {}", e.to_string()))?;

        Ok(())
    }
}

/// Build a standalone binary from MIR modules.
pub fn build_binary<'ctx>(
    context: &'ctx Context,
    modules: &[(&str, &MirModule)],
    config: BinaryBuilderConfig,
) -> Result<PathBuf, String> {
    let mut builder = BinaryBuilder::new(context, config);

    for (name, mir) in modules {
        builder.add_module(name, mir)?;
    }

    builder.build()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_binary_builder_config_default() {
        let config = BinaryBuilderConfig::default();
        assert_eq!(config.output_path, PathBuf::from("a.out"));
        assert_eq!(config.target_triple, "x86_64-unknown-linux-gnu");
        assert!(config.static_link);
    }

    #[test]
    fn test_linker_args() {
        let context = Context::create();
        let config = BinaryBuilderConfig::default();
        let builder = BinaryBuilder::new(&context, config);
        
        let args = builder.get_linker_args();
        assert!(!args.contains(&"-static".to_string())); // No longer using static
        assert!(args.contains(&"-lpthread".to_string()));
        assert!(args.contains(&"-ljdruby_ffi".to_string()));
    }
}
