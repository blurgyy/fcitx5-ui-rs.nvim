{
  description = "Fcitx5 integration for Neovim";

  inputs = {
    nixpkgs.url = "github:nixos/nixpkgs/nixpkgs-unstable";
    flake-utils.url = "github:numtide/flake-utils";
  };

  outputs =
    {
      self,
      nixpkgs,
      flake-utils,
      ...
    }:
    flake-utils.lib.eachSystem [ "x86_64-linux" "aarch_64-linux" ] (
      system:
      let
        pkgs = import nixpkgs {
          inherit system;
          overlays = [ self.overlays.default ];
        };
      in
      {
        packages = {
          inherit (pkgs) fcitx5-ui-rs-nvim;
          default = pkgs.fcitx5-ui-rs-nvim;
        };
      }
    )
    // {
      overlays.default =
        final: prev:
        let
          version = "0.1.0";
        in
        {
          fcitx5-ui-rs-nvim = final.callPackage ./plugin {
            inherit version;
          };
        };
      hydraJobs = self.packages;
    };
}
