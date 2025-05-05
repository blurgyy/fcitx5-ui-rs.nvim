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
    flake-utils.lib.eachSystem [ "x86_64-linux" "aarch64-linux" ] (
      system:
      let
        pkgs = import nixpkgs {
          inherit system;
          overlays = [ self.overlays.default ];
        };
      in
      {
        packages = {
          inherit (pkgs.vimPlugins) fcitx5-ui-rs-nvim;
          default = pkgs.vimPlugins.fcitx5-ui-rs-nvim;
        };
      }
    )
    // {
      overlays.default =
        final: prev:
        let
          mtime = self.lastModifiedDate;
          date = "${builtins.substring 0 4 mtime}-${builtins.substring 4 2 mtime}-${builtins.substring 6 2 mtime}";
          rev = self.rev or (nixpkgs.lib.warn "Git changes are not committed" (self.dirtyRev or "dirty"));
          version = "${date}-${rev}";
        in
        {
          vimPlugins = prev.vimPlugins // {
            fcitx5-ui-rs-nvim = final.callPackage ./plugin {
              inherit version;
            };
          };
        };
      hydraJobs = self.packages;
    };
}
