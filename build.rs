use std::{path::Path, process::Command};

fn main() {
    let dashboard = Path::new("dashboard");
    let output = dashboard.join("dist");

    // Cargo must rebuild the executable whenever the dashboard source or its
    // build configuration changes. The generated `dist` files are embedded by
    // `include_bytes!` in src/dashboard.rs.
    println!("cargo:rerun-if-changed=dashboard/index.html");
    println!("cargo:rerun-if-changed=dashboard/src");
    println!("cargo:rerun-if-changed=dashboard/style.css");
    println!("cargo:rerun-if-changed=dashboard/package.json");
    println!("cargo:rerun-if-changed=dashboard/tsconfig.json");
    println!("cargo:rerun-if-changed=dashboard/vite.config.ts");
    println!("cargo:rerun-if-env-changed=npm_config_user_agent");

    if !dashboard.join("node_modules").is_dir() {
        run_npm(
            "install",
            &["install", "--no-package-lock"],
            "install dashboard dependencies",
        );
    }

    run_npm("run build", &["run", "build"], "build the dashboard");

    for required in [
        output.join("index.html"),
        output.join("assets/app.js"),
        output.join("assets/controller.js"),
        output.join("assets/style.css"),
    ] {
        if !required.is_file() {
            panic!(
                "Dashboard build did not produce required embedded asset: {}",
                required.display()
            );
        }
    }
}

fn run_npm(label: &str, args: &[&str], description: &str) {
    let status = Command::new("npm")
        .args(args)
        .current_dir("dashboard")
        .status()
        .unwrap_or_else(|error| {
            panic!(
                "Could not run npm to {description}: {error}. Install Node.js and npm, then retry cargo build."
            )
        });

    if !status.success() {
        panic!("npm {label} failed with status {status}. See the npm output above.");
    }
}
