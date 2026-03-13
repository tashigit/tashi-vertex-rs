use std::env;

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

    // Copy libraries to the target output directory
    let profile = env::var("PROFILE")?;
    let _ = copy_to_output_path(&vertex, &profile);

    Ok(())
}
