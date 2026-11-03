fn main() {
    let src = std::path::Path::new("src");
    cc::Build::new()
        .std("c11")
        .include(src)
        .file(src.join("parser.c"))
        .file(src.join("scanner.c"))
        .warnings(false)
        .compile("tree-sitter-plsql");
    for f in ["parser.c", "scanner.c"] {
        println!("cargo:rerun-if-changed=src/{f}");
    }
}
