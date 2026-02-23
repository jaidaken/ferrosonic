{
  inputs.nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";

  outputs = inputs@{ self, nixpkgs, ... }:
  let
    system = "x86_64-linux";
    pkgs = nixpkgs.legacyPackages.${system};
  in
  {
    packages.${system} = rec {
      ferrosonic = pkgs.callPackage ./ferrosonic.nix { };
      default = ferrosonic;
    };

    devShells.${system}.default = (import ./shell.nix { inherit pkgs; });
  };
}
