{
  pkgs ? import <nixpkgs> { },
}:
let
  runtimeLibs = with pkgs; [
    alsa-lib
    libGL
    libxkbcommon
    wayland
    libpulseaudio

    libX11
    libXcursor
    libXi
    libXrandr
  ];
in
pkgs.mkShell {
  nativeBuildInputs = with pkgs; [
    rustc
    rust-analyzer
    clippy
    cargo
    rustfmt
    gnumake
    shfmt
    llvm
    pkg-config
    lldb
    pulseaudio
    (pkgs.writeShellScriptBin "codelldb" ''
      exec ${pkgs.vscode-extensions.vadimcn.vscode-lldb}/share/vscode/extensions/vadimcn.vscode-lldb/adapter/codelldb "$@"
    '')
  ];

  buildInputs = runtimeLibs;

  shellHook = ''
    # Injeta os caminhos das bibliotecas do /nix/store para o dlopen do winit encontrar
    export LD_LIBRARY_PATH="${pkgs.lib.makeLibraryPath runtimeLibs}:$LD_LIBRARY_PATH"
    echo "Rust env ok (Wayland & Audio prontos)"
  '';
}
