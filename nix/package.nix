{ inputs, pkgs, ... }:
let
  crane = import ./crane.nix { inherit inputs pkgs; };
in
crane.craneLib.buildPackage (
  crane.commonArgs
  // {
    inherit (crane) cargoArtifacts;
    cargoExtraArgs = "-p oracli";
    # The checks run the tests.
    doCheck = false;
    meta.mainProgram = "oracli";
  }
)
