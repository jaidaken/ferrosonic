{
  description = "A terminal Subsonic music client written in Rust: bit-perfect audio, gapless playback, and full desktop integration.";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    naersk = {
      url = "github:nix-community/naersk";
      inputs.nixpkgs.follows = "nixpkgs";
    };
    fenix = {
      url = "github:nix-community/fenix";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs = {
    self,
    nixpkgs,
    naersk,
    fenix,
  }: let
    system = "x86_64-linux";
    pkgs = import nixpkgs {inherit system;};
    fenixLib = fenix.packages.${system};
    rustToolchain = fenixLib.default.toolchain;
  in {
    devShell.${system} = pkgs.mkShell {
      buildInputs = with pkgs; [
        rustToolchain
      ];

      nativeBuildInputs = [pkgs.pkg-config];
      shellHook = ''
        cargo -V
      '';
    };

    packages.${system}.default =
      (naersk.lib.x86_64-linux.override {
        cargo = rustToolchain;
        rustc = rustToolchain;
      }).buildPackage {
        pname = "ferrosonic";
        version = "0.6.1";
        src = ./.;
      };
  };
}
