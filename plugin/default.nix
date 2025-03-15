{
  lib,
  version,
  rustPlatform,

  clippy,
  pkg-config,
  dbus,
}:

rustPlatform.buildRustPackage {
  pname = "fcitx5-ui-rs.nvim";
  inherit version;
  src = ./.;

  nativeBuildInputs = [
    clippy
    pkg-config
    rustPlatform.bindgenHook # solves: libclang.so not found
  ];
  buildInputs = [
    dbus.dev
  ];

  shellHook = ''
    [[ "$-" == *i* ]] && exec $(grep -E "^$USER:" /etc/passwd | awk -F: '{ print $NF }')
  '';

  cargoLock.lockFile = ./Cargo.lock;

  meta = {
    description = "Fcitx5 integration for Neovim";
    license = lib.licenses.gpl3;
    homepage = "https://github.com/blurgyy/fcitx5-ui-rs.nvim";
  };
}
