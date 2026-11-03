{ inputs, pkgs, ... }:
let
  crane = import ../crane.nix { inherit inputs pkgs; };
in
# Ignored tests need a database and stay out of the sandbox.
crane.craneLib.cargoTest (crane.commonArgs // { inherit (crane) cargoArtifacts; })
