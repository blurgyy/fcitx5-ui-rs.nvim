{
  version,
  rustPlatform,

  pkg-config,
  luajit,
  dbus,
}:

rustPlatform.buildRustPackage {
  pname = "fcitx5-ui-rs.nvim-lib";
  inherit version;
  src = ./.;

  nativeBuildInputs = [
    rustPlatform.bindgenHook
    pkg-config
  ];
  buildInputs = [
    luajit
    dbus.dev
  ];

  shellHook = ''
    [[ "$-" == *i* ]] && exec $(grep -E "^$USER:" /etc/passwd | awk -F: '{ print $NF }')
  '';

  cargoLock.lockFile = ./Cargo.lock;
}
