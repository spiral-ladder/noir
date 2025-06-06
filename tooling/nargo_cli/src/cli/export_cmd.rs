use nargo::errors::CompileError;
use nargo::ops::{check_crate_and_report_errors, report_errors};
use noir_artifact_cli::fs::artifact::save_program_to_file;
use noirc_errors::CustomDiagnostic;
use noirc_frontend::hir::ParsedFiles;
use rayon::prelude::*;

use fm::FileManager;
use iter_extended::try_vecmap;
use nargo::package::Package;
use nargo::prepare_package;
use nargo::workspace::Workspace;
use nargo::{insert_all_files_for_workspace_into_file_manager, parse_all};
use nargo_toml::PackageSelection;
use noirc_driver::{CompileOptions, CompiledProgram, compile_no_check};

use clap::Args;

use crate::errors::CliError;

use super::{LockType, PackageOptions, WorkspaceCommand};

#[allow(rustdoc::broken_intra_doc_links)]
/// Exports functions marked with #[export] attribute
#[derive(Debug, Clone, Args)]
pub(crate) struct ExportCommand {
    #[clap(flatten)]
    pub(super) package_options: PackageOptions,

    #[clap(flatten)]
    compile_options: CompileOptions,
}

impl WorkspaceCommand for ExportCommand {
    fn package_selection(&self) -> PackageSelection {
        self.package_options.package_selection()
    }

    fn lock_type(&self) -> LockType {
        // Writes the exported functions.
        LockType::Exclusive
    }
}

pub(crate) fn run(args: ExportCommand, workspace: Workspace) -> Result<(), CliError> {
    let mut workspace_file_manager = workspace.new_file_manager();
    insert_all_files_for_workspace_into_file_manager(&workspace, &mut workspace_file_manager);
    let parsed_files = parse_all(&workspace_file_manager);

    let library_packages: Vec<_> =
        workspace.into_iter().filter(|package| package.is_library()).collect();

    library_packages
        .par_iter()
        .map(|package| {
            compile_exported_functions(
                &workspace_file_manager,
                &parsed_files,
                &workspace,
                package,
                &args.compile_options,
            )
        })
        .collect()
}

fn compile_exported_functions(
    file_manager: &FileManager,
    parsed_files: &ParsedFiles,
    workspace: &Workspace,
    package: &Package,
    compile_options: &CompileOptions,
) -> Result<(), CliError> {
    let (mut context, crate_id) = prepare_package(file_manager, parsed_files, package);
    check_crate_and_report_errors(&mut context, crate_id, compile_options)?;

    let exported_functions = context.get_all_exported_functions_in_crate(&crate_id);

    let exported_programs = try_vecmap(
        exported_functions,
        |(function_name, function_id)| -> Result<(String, CompiledProgram), CompileError> {
            // TODO: We should to refactor how to deal with compilation errors to avoid this.
            let program = compile_no_check(&mut context, compile_options, function_id, None, false)
                .map_err(|error| vec![CustomDiagnostic::from(error)]);

            let program = report_errors(
                program.map(|program| (program, Vec::new())),
                file_manager,
                compile_options.deny_warnings,
                compile_options.silence_warnings,
            )?;

            Ok((function_name, program))
        },
    )?;

    let export_dir = workspace.export_directory_path();
    for (function_name, program) in exported_programs {
        save_program_to_file(&program.into(), &function_name.parse().unwrap(), &export_dir)?;
    }
    Ok(())
}
