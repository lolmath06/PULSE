//! Run the isolated native pixel probe with PULSE's actual startup renderer
//! selection. No application state, metrics providers or existing PULSE
//! processes are accessed. Pass --serve to start/stop a dedicated Vite child.
fn main() {
    pulse_lib::platform::prepare_runtime_environment();
    let script =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../scripts/test-overlay-webkit.py");
    let status = std::process::Command::new("python3")
        .arg(script)
        .args(std::env::args_os().skip(1))
        .status()
        .expect("run WebKitGTK probe");
    std::process::exit(status.code().unwrap_or(1));
}
