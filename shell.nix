{ pkgs ? import <nixpkgs> { } }:
pkgs.mkShell {
  packages = with pkgs; [
    rustc
    cargo
    pkg-config
    openssl
    mpv
    pipewire
    wireplumber
    cava
  ];
}
