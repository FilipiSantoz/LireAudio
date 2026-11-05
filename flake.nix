{
  description = "Lire Audio";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
  };

  outputs =
    { self, nixpkgs }:
    let
      system = "x86_64-linux";
      pkgs = nixpkgs.legacyPackages.${system};

      runtimeLibs = with pkgs; [
        alsa-lib
        libGL
        libxkbcommon
        wayland
        libX11
        libXcursor
        libXi
        libXrandr
        vulkan-loader
      ];
    in
    {
      devShells.${system}.default = pkgs.mkShell {

        nativeBuildInputs = with pkgs; [
          rustc
          rust-analyzer
          clippy
          cargo
          rustfmt
          shfmt
          llvm
          pkg-config
          nodejs
          bun
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
          llvmPackages.libclang.lib
          rustPlatform.bindgenHook
        ];

        buildInputs = runtimeLibs ++ [
          pkgs.pipewire
        ];

        shellHook = ''
          export LD_LIBRARY_PATH="/run/opengl-driver/lib:${pkgs.lib.makeLibraryPath runtimeLibs}:$LD_LIBRARY_PATH"

          export XDG_DATA_DIRS="/run/opengl-driver/share:${pkgs.gsettings-desktop-schemas}/share/gsettings-schemas/${pkgs.gsettings-desktop-schemas.name}:${pkgs.gtk3}/share/gsettings-schemas/${pkgs.gtk3.name}:${pkgs.adwaita-icon-theme}/share:${pkgs.hicolor-icon-theme}/share:$XDG_DATA_DIRS"
          export GIO_MODULE_DIR="${pkgs.glib}/lib/gio/modules"
          export BINDGEN_EXTRA_CLANG_ARGS="$BINDGEN_EXTRA_CLANG_ARGS $(pkg-config --cflags libspa-0.2)"
          export GDK_BACKEND=wayland
        '';
      };
    };
}
