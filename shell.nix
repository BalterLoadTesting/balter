{ pkgs ? import <nixpkgs> {} }:
with pkgs; mkShell {
    buildInputs = with pkgs; [
      clang
      # Replace llvmPackages with llvmPackages_X, where X is the latest LLVM version (at the time of writing, 16)
      llvmPackages.bintools
      # bintools-unwrapped
      # rustup
      cargo
      rustc
      just
      fastmod
      glibc
      cpulimit
      gcc
      clippy
      linuxPackages_latest.perf
      hyperfine
      openssl
      pkg-config
      prometheus
      grafana
    ];
}
