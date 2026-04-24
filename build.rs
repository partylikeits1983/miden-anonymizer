//! Compiles the PTA MASM sources under `./masm` into Miden assembly libraries
//! (`.masl` files) placed under `$OUT_DIR/assets`.
//!
//! This is a stripped-down port of `miden-base/crates/miden-standards/build.rs`.
//! Two artifacts are produced:
//!
//! 1. `$OUT_DIR/assets/standards.masl` - compiled from `masm/standards/**.masm`,
//!    namespaced under `miden::pta::standards`. This library contains the
//!    P2IDF note script. It links against `miden::standards::notes::p2id`
//!    and `miden::standards::wallets::basic` from miden-standards.
//!
//! 2. `$OUT_DIR/assets/account_components/auth/vault_empty.masl` - compiled
//!    from `masm/account_components/auth/vault_empty.masm`, with the library
//!    path `miden::pta::components::auth::vault_empty`.

use std::env;
use std::path::Path;
use std::sync::Arc;

use fs_err as fs;
use miden_assembly::diagnostics::{IntoDiagnostic, NamedSource, Result};
use miden_assembly::{Assembler, Library};
use miden_protocol::transaction::TransactionKernel;
use walkdir::WalkDir;

// CONSTANTS
// ================================================================================================

const ASSETS_DIR: &str = "assets";
const ASM_DIR: &str = "masm";
const ASM_STANDARDS_DIR: &str = "standards";
const ASM_ACCOUNT_COMPONENTS_DIR: &str = "account_components";

const STANDARDS_LIB_NAMESPACE: &str = "miden::pta::standards";
const ACCOUNT_COMPONENTS_LIB_NAMESPACE: &str = "miden::pta::components";

// BUILD ENTRYPOINT
// ================================================================================================

fn main() -> Result<()> {
    println!("cargo::rerun-if-changed={ASM_DIR}/");

    let crate_dir = env::var("CARGO_MANIFEST_DIR").unwrap();
    let build_dir = env::var("OUT_DIR").unwrap();

    let source_dir = Path::new(&crate_dir).join(ASM_DIR);
    let target_dir = Path::new(&build_dir).join(ASSETS_DIR);

    // Base assembler: the transaction kernel assembler, so kernel procs
    // (active_account::*, active_note::*, asset::*, native_account::*, word::*, ...)
    // resolve automatically.
    let mut assembler = TransactionKernel::assembler().with_warnings_as_errors(true);

    // Link against miden-standards' compiled library so `wallet::receive_asset`,
    // `wallet::move_asset_to_note`, etc. resolve at assembly time. Static
    // linking inlines the referenced procedures' MAST into our artifact, so
    // the runtime doesn't need separate resolution.
    let standards_lib = miden_standards::StandardsLib::default();
    assembler.link_static_library(standards_lib.as_ref().clone())?;

    // 1. Compile our standards library (P2IDF note script, etc.).
    let pta_standards_lib = compile_standards_lib(&source_dir, &target_dir, assembler.clone())?;
    assembler.link_static_library(pta_standards_lib)?;

    // 2. Compile our account components (VaultEmptyAuth).
    compile_account_components(
        &source_dir.join(ASM_ACCOUNT_COMPONENTS_DIR),
        &target_dir.join(ASM_ACCOUNT_COMPONENTS_DIR),
        assembler,
    )?;

    Ok(())
}

// COMPILE PTA STANDARDS LIB
// ================================================================================================

fn compile_standards_lib(
    source_dir: &Path,
    target_dir: &Path,
    assembler: Assembler,
) -> Result<Library> {
    let source_dir = source_dir.join(ASM_STANDARDS_DIR);
    let lib = assembler.assemble_library_from_dir(source_dir, STANDARDS_LIB_NAMESPACE)?;

    if !target_dir.exists() {
        fs::create_dir_all(target_dir).unwrap();
    }
    let output_file = target_dir
        .join("standards")
        .with_extension(Library::LIBRARY_EXTENSION);
    lib.write_to_file(output_file).into_diagnostic()?;

    Ok(Arc::unwrap_or_clone(lib))
}

// COMPILE ACCOUNT COMPONENTS
// ================================================================================================

fn compile_account_components(
    source_dir: &Path,
    target_dir: &Path,
    assembler: Assembler,
) -> Result<()> {
    if !target_dir.exists() {
        fs::create_dir_all(target_dir).unwrap();
    }

    for masm_file_path in get_masm_files(source_dir)? {
        let component_name = masm_file_path
            .file_stem()
            .expect("masm file should have a file stem")
            .to_str()
            .expect("file stem should be valid UTF-8")
            .to_owned();

        let component_source_code =
            fs::read_to_string(&masm_file_path).expect("reading component MASM should succeed");

        // Derive the namespaced library path from the directory layout.
        // e.g. auth/vault_empty.masm -> miden::pta::components::auth::vault_empty
        let relative_path = masm_file_path
            .strip_prefix(source_dir)
            .expect("masm file should be inside source dir");
        let mut library_path = ACCOUNT_COMPONENTS_LIB_NAMESPACE.to_owned();
        for component in relative_path.with_extension("").components() {
            let part = component.as_os_str().to_str().expect("valid UTF-8");
            library_path.push_str("::");
            library_path.push_str(part);
        }

        let named_source = NamedSource::new(library_path, component_source_code);
        let component_library = assembler
            .clone()
            .assemble_library([named_source])
            .expect("component library assembly should succeed");

        // Preserve the subdirectory structure on disk.
        let relative_dir = masm_file_path
            .parent()
            .and_then(|p| p.strip_prefix(source_dir).ok())
            .unwrap_or(Path::new(""));

        let output_dir = target_dir.join(relative_dir);
        if !output_dir.exists() {
            fs::create_dir_all(&output_dir).unwrap();
        }

        let component_file_path = output_dir
            .join(component_name)
            .with_extension(Library::LIBRARY_EXTENSION);
        component_library
            .write_to_file(component_file_path)
            .into_diagnostic()?;
    }

    Ok(())
}

// HELPERS
// ================================================================================================

fn get_masm_files<P: AsRef<Path>>(dir: P) -> Result<Vec<std::path::PathBuf>> {
    let mut files = Vec::new();
    let path = dir.as_ref();
    if path.is_dir() {
        for entry in WalkDir::new(path) {
            let entry = entry.into_diagnostic()?;
            let file_path = entry.path().to_path_buf();
            if file_path.extension().and_then(|e| e.to_str()) == Some("masm") {
                files.push(file_path);
            }
        }
    }
    Ok(files)
}
