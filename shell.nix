{
  pkgs ? import <nixpkgs> { },
}:
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
    (pkgs.writeShellScriptBin "codelldb" ''
      exec ${pkgs.vscode-extensions.vadimcn.vscode-lldb}/share/vscode/extensions/vadimcn.vscode-lldb/adapter/codelldb "$@"
    '')
  ];

  buildInputs = with pkgs; [
    alsa-lib
  ];

  shellHook = ''
    echo "Rust env ok"
  '';
}
