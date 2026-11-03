# Shared by the package and the checks so they reuse one build of the dependencies.
{ inputs, pkgs }:
let
  inherit (pkgs) lib;
  craneLib = (inputs.crane.mkLib pkgs).overrideScope (
    _: _: {
      stdenvSelector =
        p: if p.stdenv.hostPlatform.isLinux then p.stdenvAdapters.useMoldLinker p.stdenv else p.stdenv;
    }
  );
  root = ../.;
  commonArgs = {
    src = lib.fileset.toSource {
      inherit root;
      fileset = lib.fileset.unions [
        (craneLib.fileset.commonCargoSources root)
        # insta snapshots
        (lib.fileset.fileFilter (f: f.hasExt "snap") root)
        # the generated tree-sitter parser
        (lib.fileset.fileFilter (
          f: f.hasExt "c" || f.hasExt "h" || f.name == "node-types.json"
        ) ../crates/tree-sitter-plsql/src)
      ];
    };
    inherit (craneLib.crateNameFromCargoToml { cargoToml = ../crates/oracli/Cargo.toml; })
      pname
      version
      ;
    strictDeps = true;
    # aws-lc-sys, pulled in by oracle-rs's TLS stack
    nativeBuildInputs = [ pkgs.cmake ];
  };
in
{
  inherit craneLib commonArgs;
  cargoArtifacts = craneLib.buildDepsOnly commonArgs;
}
