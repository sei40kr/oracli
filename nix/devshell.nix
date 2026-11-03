{ inputs, pkgs, ... }:
let
  pre-commit-check = import ./checks/pre-commit-check.nix { inherit inputs pkgs; };
  mkShell =
    if pkgs.stdenv.hostPlatform.isLinux then
      pkgs.mkShell.override { stdenv = pkgs.stdenvAdapters.useMoldLinker pkgs.stdenv; }
    else
      pkgs.mkShell;
in
mkShell {
  packages = with pkgs; [
    cargo
    rustc
    clippy
    rustfmt
    rust-analyzer
    cargo-insta
    cmake
    mold
    sccache
    # regenerating crates/tree-sitter-plsql/src from grammar.js
    tree-sitter
    nodejs
  ];

  env = {
    RUSTC_WRAPPER = "sccache";
    RUST_SRC_PATH = "${pkgs.rustPlatform.rustLibSrc}";
  };

  shellHook = ''
    ${pre-commit-check.shellHook}
  '';
}
