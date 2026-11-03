{ inputs, pkgs, ... }:
let
  crane = import ../crane.nix { inherit inputs pkgs; };
in
crane.craneLib.cargoClippy (
  crane.commonArgs
  // {
    inherit (crane) cargoArtifacts;
    cargoClippyExtraArgs = "--all-targets -- -D warnings";
  }
)
