#[cfg(feature = "lightstep")]
fn main() {
    use std::process::Command;

    // Ensure this script is executed again, if the lightstep-tracer-common
    // directory changes.
    println!("rerun-if-changed=lightstep-tracer-common");

    Command::new("git")
        .args(&[
            "clone",
            // Clone at tag v1.0.3
            "--branch=v1.0.3",
            // Clone only one commit to speed up the process
            "--depth=1",
            "https://github.com/lightstep/lightstep-tracer-common.git",
        ])
        .output()
        .expect("Cloning lightstep/lightstep-tracer-common repository failed");

    prost_build::compile_protos(
        &[
            "lightstep-tracer-common/collector.proto",
            "lightstep-tracer-common/lightstep.proto",
        ],
        &[
            "lightstep-tracer-common",
            "lightstep-tracer-common/third_party/googleapis",
        ],
    )
    .unwrap();
}

#[cfg(not(feature = "lightstep"))]
fn main() {}
