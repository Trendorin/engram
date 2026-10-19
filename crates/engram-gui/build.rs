fn main() {
    // "native" resolves to Slint's Qt style when the Qt backend was built (Qt 6
    // found through qmake), and to fluent otherwise. SLINT_STYLE overrides it.
    let mut config = slint_build::CompilerConfiguration::new();
    if std::env::var_os("SLINT_STYLE").is_none() {
        config = config.with_style("native".into());
    }
    slint_build::compile_with_config("ui/app.slint", config).expect("compile ui/app.slint");
}
