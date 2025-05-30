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

  cargoLock = {
    lockFile = ./Cargo.lock;
    outputHashes = {
      # TODO: use upstream again once <https://github.com/noib3/nvim-oxi/issues/226> is
      # resolved
      "nvim-oxi-0.5.1" = "sha256-IvS2uu0IcgYVpvAaqJeZ920mvDA3FydexF/ON9FpXVQ=";
    };
  };

  postInstall = ''
    mkdir $out/lua -p
    mv $out/lib/libfcitx5_ui_rs.so $out/lua/fcitx5_ui_rs.so
    rm -rv $out/lib
  '';

  meta = {
    description = "Fcitx5 integration for Neovim";
    license = lib.licenses.gpl3;
    homepage = "https://github.com/blurgyy/fcitx5-ui-rs.nvim";
  };
}
