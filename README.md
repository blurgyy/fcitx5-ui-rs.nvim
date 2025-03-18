# fcitx5-ui-rs.nvim

This plugin allows using the [Fcitx5] input method natively inside Neovim (v0.10+).

You can now use Fcitx5 remotely in an SSH session!

![demo](https://github.com/user-attachments/assets/6c500b57-58ab-4ae8-bfca-54ff00755c5f)

<details>
<summary>See video</summary>

https://github.com/user-attachments/assets/23e34a5a-ae3b-4531-bd3d-8786fbea6695

</details>

## Installation

I personally use [Nix] for plugin management, but a general installation guide and an
example that works on a bare-bones nvim instance are provided here.
Please feel free to open a PR to add better out-of-the-box installation support for your
favorite plugin manager.

<details>
<summary>Click to expand</summary>

### General

This plugin is a single .so binary.  A vanilla installation process would be:

1. Build the plugin with `cargo build --release`, this gives you a binary at
   `target/release/libfcitx5_ui_rs.so`.
2. The binary needs to be renamed to `lua/fcitx5_ui_rs.so` (remove the "`lib`" prefix),
   and put inside your Neovim's [`rtp`].

For example, here is how to test this plugin imperatively:

```bash
cd fcitx5-ui-rs.nvim/plugin
cargo build --release  # <-- This gives target/release/libfcitx5_ui_rs.so
mkdir lua
mv ./target/release/libfcitx5_ui_rs.so lua/fcitx5_ui_rs.so  # <-- NB: must remove the `lib` prefix!
# Then launch nvim.
# We add $(pwd) to nvim's rtp so that it can find lua/fcitx5_ui_rs.so.
nvim +"set rtp^=$(pwd)" +'lua require("fcitx5_ui_rs").setup({
  on_key = "<M-Space>",
})'  # See the configuration guide below
# Then hit Alt+Space inside nvim, and try typing something!
```

### NixOS

Add this project's to your flake's input:

```nix
{
  inputs.fcitx5-ui-rs-nvim.url = "github:blurgyy/fcitx5-ui-rs.nvim";
  # ...
}
```

Add to your nixpkgs's overlay:

```nix
  pkgs = import nixpkgs {
    overlays = [
      # ...
      inputs.fcitx5-ui-rs-nvim.overlays.default
    ];
  };
```

The plugin can now be built via `pkgs.vimPlugins.fcitx5-ui-rs-nvim`.  You can then add
it to your Neovim plugins.

</details>

## Configuration

To load the plugin, call `require('fcitx5_ui_rs').setup({})`:

```lua
require('fcitx5_ui_rs').setup({
  on_key = "<M-Space>",  -- Use Alt+Space to activate/deactivate input method.  Default is nil
  im_active = "pinyin",  -- Your active input method name, see $XDG_CONFIG_HOME/fcitx5/profile.  Default is "pinyin"
  im_inactive = "keyboard-us",  -- Your inactive input method name, see $XDG_CONFIG_HOME/fcitx5/profile.  Default is "keyboard-us"
})
```

### Showing current IM on [lualine]

```lua
function lualine_get_im()
  local im = require("fcitx5_ui_rs").get_im()
  local mapping = {
    [""] = "",
    ["keyboard-us"] = " ",
    ["pinyin"] = "中",
  }
  if mapping[im] then
    return mapping[im]
  else
    return "? " -- unrecognized, shouldn't ever be seen
  end
end

local cfg = require('lualine').get_config()
table.insert(
  cfg.sections.lualine_y,
  'lualine_get_im()'
)
require('lualine').setup(cfg)
```

## Limitations

This plugin depends on [Fcitx5]'s DBus frontend, it would not work on a system without
DBus.

## Known Problem

- Special characters like `#`, <code>\`</code>, `$`, `%`, etc. are inserted after space,
  if they are selected with a space key.

## Thanks

This project would not be possible without the following projects:

- [fcitx5-ui.nvim]: Integrates Fcitx5 via DBus, but using lua.  This project was
  inspired by it, but solves various lua dependency problems on NixOS.
- [nvim-oxi]: Provides Rust bindings for Neovim internals.
- [fcitx5-dbus]: Provides DBus interface for Fcitx5 in Rust.

## Contribution

Contributions are welcome.  Feel free to send issues or PRs!

## License

[GPL-3.0].

[Fcitx5]: <https://fcitx-im.org/wiki/Fcitx_5>
[lualine]: <https://github.com/nvim-lualine/lualine.nvim>
[Nix]: <https://nixos.org>
[`rtp`]: <https://neovim.io/doc/user/options.html#'runtimepath'>
[fcitx5-ui.nvim]: <https://github.com/black-desk/fcitx5-ui.nvim>
[nvim-oxi]: <https://github.com/noib3/nvim-oxi>
[fcitx5-dbus]: <https://github.com/Jedsek/fcitx5-dbus>
[GPL-3.0]: <./LICENSE>
