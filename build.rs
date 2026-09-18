use std::{env, fs, path::PathBuf};

fn main() {
    let manifest_dir =
        PathBuf::from(env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR is not set"));

    let dashboard_dir = manifest_dir.join("dashboard");

    let out_dir = PathBuf::from(env::var("OUT_DIR").expect("OUT_DIR is not set"));

    let generated = out_dir.join("dashboard_assets.rs");

    let files = [
        ("index.html", "text/html; charset=utf-8"),
        ("app.js", "application/javascript; charset=utf-8"),
        ("style.css", "text/css; charset=utf-8"),
    ];

    let mut output = String::new();

    output.push_str("pub static DASHBOARD_ASSETS: &[(&str, &str, &str)] = &[\n");

    for (filename, content_type) in files {
        let path = dashboard_dir.join(filename);

        println!("cargo:rerun-if-changed={}", path.display());

        let contents = fs::read_to_string(&path)
            .unwrap_or_else(|error| panic!("failed to read {}: {}", path.display(), error));

        let escaped = contents.escape_default().to_string();

        output.push_str(&format!(
            "    ({:?}, {:?}, {:?}),\n",
            filename, content_type, escaped
        ));
    }

    output.push_str("];\n");

    fs::write(&generated, output).expect("failed to write dashboard_assets.rs");
}
