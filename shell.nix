let
  # pkgs = import <nixpkgs> { };
  pkgs = import (fetchTarball https://nixos.org/channels/nixos-unstable/nixexprs.tar.xz) { };
in
pkgs.mkShell {
  nativeBuildInputs = with pkgs; [
    pkg-config
    # glib
    # glibc
    # libstdcxx5
  ];
  buildInputs = with pkgs;[
    # cargo
    alsa-lib
  ];
  shellHook = ''
    export LD_LIBRARY_PATH="$LD_LIBRARY_PATH:${
      pkgs.lib.makeLibraryPath  [
        # pkgs.stdenv.cc.cc.lib
        # pkgs.glib
        # pkgs.glibc
        # pkgs.libstdcxx5
      ]
    }"'';
}
