{ pkgs ? import <nixpkgs> {} }:
with pkgs;
mkShell {
  nativeBuildInputs = [
    bintools-unwrapped
    just
    fastmod
    glibc
    cpulimit
    rustc
    cargo
    gcc
    rustfmt
    clippy
  ];

  RUST_SRC_PATH = "${pkgs.rust.packages.stable.rustPlatform.rustLibSrc}";
}
