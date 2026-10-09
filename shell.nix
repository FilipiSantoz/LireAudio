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
    nodejs
    pulseaudio
    gtk4
    cairo
    gdk-pixbuf
    glib
    wrapGAppsHook4
    webkitgtk_6_0
    librsvg
    (pkgs.writeShellScriptBin "codelldb" ''
      exec ${pkgs.vscode-extensions.vadimcn.vscode-lldb}/share/vscode/extensions/vadimcn.vscode-lldb/adapter/codelldb "$@"
    '')
  ];

  buildInputs = runtimeLibs;

  shellHook = ''
    export LD_LIBRARY_PATH="${pkgs.lib.makeLibraryPath runtimeLibs}:$LD_LIBRARY_PATH"
  '';
}
