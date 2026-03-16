use std::env;
use std::process::Command;

use copy_to_output::copy_to_output_path;

fn main() -> anyhow::Result<()> {
    // Build the CMake project located in the current directory
    // This will fetch the pre-built libraries as specified in the CMakeLists.txt
    // and link them to the Rust project.
    let vertex = cmake::build("").join("lib");

    // Declare the path to the pre-built libraries for linking
    println!("cargo:rustc-link-search=native={}", vertex.display());

    // Declare a dynamic link dependency on the tashi_vertex library
    println!("cargo:rustc-link-lib=dylib=tashi-vertex");

    
    if env::var("CARGO_CFG_TARGET_OS")? == "macos" {
        let dylib_path = vertex.join("libtashi-vertex.dylib");
        if dylib_path.exists() {
            // Fix the internal ID of the library so it's relocatable via @rpath
            Command::new("install_name_tool")
                .arg("-id")
                .arg("@rpath/libtashi-vertex.dylib")
                .arg(&dylib_path)
                .status()?;
        }

        // Tell the linker to add rpaths to any binary (example, test, or lib) that links to this crate.
        // @loader_path refers to the directory containing the binary.
        println!("cargo:rustc-link-arg=-Wl,-rpath,@loader_path/../lib"); // for examples
        println!("cargo:rustc-link-arg=-Wl,-rpath,@loader_path/../../lib"); // for tests in deps/
    }

    // Copy libraries to the target output directory
    let profile = env::var("PROFILE")?;
    let _ = copy_to_output_path(&vertex, &profile);

    Ok(())
}
