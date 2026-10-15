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
    jq
    gnumake
    shfmt
    llvm
    pkg-config
    lldb
    nodejs
    bun
    pulseaudio
    cairo
    gdk-pixbuf
    glib
    wrapGAppsHook3
    webkitgtk_4_1
    librsvg
    dbus
    at-spi2-core
    gtk3
    gsettings-desktop-schemas
    xrdb
    gtk4
    helvum
    (pkgs.writeShellScriptBin "codelldb" ''
      exec ${pkgs.vscode-extensions.vadimcn.vscode-lldb}/share/vscode/extensions/vadimcn.vscode-lldb/adapter/codelldb "$@"
    '')
  ];

  buildInputs = runtimeLibs;

  shellHook = ''
    export LD_LIBRARY_PATH="${pkgs.lib.makeLibraryPath runtimeLibs}:$LD_LIBRARY_PATH"

    export XDG_DATA_DIRS="$XDG_DATA_DIRS\
    :${pkgs.gsettings-desktop-schemas}/share/gsettings-schemas/${pkgs.gsettings-desktop-schemas.name}\
    :${pkgs.gtk3}/share/gsettings-schemas/${pkgs.gtk3.name}\
    :${pkgs.adwaita-icon-theme}/share\
    :${pkgs.hicolor-icon-theme}/share"
    export GIO_MODULE_DIR="${pkgs.glib}/lib/gio/modules"

    export GDK_BACKEND=wayland
  '';
}
