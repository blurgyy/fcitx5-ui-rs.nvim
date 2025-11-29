{
  lib,
  version,
  rustPlatform,

  cargo-edit,
  clippy,
  rustfmt,

  pkg-config,
  dbus,
}:

rustPlatform.buildRustPackage {
  pname = "fcitx5-ui-rs.nvim";
  inherit version;
  src = ./.;

  cargoLock = {
    lockFile = ./Cargo.lock;
    outputHashes."nvim-oxi-0.6.0" = "sha256-ak7hbl0swzaQT3AFzjbS4TollhfWLbMkEYuP4S2o6+I=";
  };

  nativeBuildInputs = [
    cargo-edit
    clippy
    rustfmt
    pkg-config
    rustPlatform.bindgenHook # solves: libclang.so not found
  ];
  buildInputs = [
    dbus.dev
  ];

  shellHook = ''
    [[ "$-" == *i* ]] && exec $(grep -E "^$USER:" /etc/passwd | awk -F: '{ print $NF }')
  '';

  postInstall = ''
    mkdir $out/lua -p
    mv $out/lib/libfcitx5_ui_rs.so $out/lua/fcitx5_ui_rs.so
    rm -rvf $out/lib $out/bin
  '';

  meta = {
    description = "Fcitx5 integration for Neovim";
    license = lib.licenses.gpl3;
    homepage = "https://github.com/blurgyy/fcitx5-ui-rs.nvim";
  };
}
