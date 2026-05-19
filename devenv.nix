{
  pkgs,
  lib,
  config,
  inputs,
  ...
}:

{
  packages = with pkgs; [
    cargo-edit
    pkg-config
    llvmPackages.libclang
    llvmPackages.libcxxClang
    clang
    opencv
  ];

  languages.rust.enable = true;
  languages.c.enable = true;
  languages.cplusplus.enable = true;

  env.LIBCLANG_PATH = "${pkgs.llvmPackages.libclang.lib}/lib";
  env.RUSTFLAGS = lib.mkForce "-C link-args=-Wl,-fuse-ld=mold,-rpath,${
    with pkgs;
    lib.makeLibraryPath [
      libGL
      libxkbcommon
      wayland
      xorg.libX11
      xorg.libXcursor
      xorg.libXi
      xorg.libXrandr
    ]
  }";
}
