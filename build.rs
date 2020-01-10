#[cfg(feature = "lightstep")]
mod implementation {
    use std::process::Command;

    fn get_rustc_version() -> String {
        let executable = std::env::var("RUSTC").unwrap_or_else(|_| "rustc".to_string());
        let output = Command::new(executable).arg("-V").output().unwrap();
        String::from_utf8(output.stdout).unwrap().trim().to_string()
    }

    pub fn main() {
        // Used for the lightstep.tracer_platform_version tag
        println!("cargo:rustc-env=RUSTC_VERSION=\"{}\"", get_rustc_version());

        // Ensure this script is executed again, if the lightstep-tracer-common
        // directory changes.
        println!("cargo:rerun-if-changed=lightstep-tracer-common");

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
}

#[cfg(not(feature = "lightstep"))]
mod implementation {
    pub fn main() {}
}

fn main() {
    implementation::main();
}
