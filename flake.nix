{
  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    rust-overlay.url = "github:oxalica/rust-overlay";
  };

  outputs =
    {
      nixpkgs,
      rust-overlay,
      ...
    }:
    let
      system = "x86_64-linux"; # darwin 👎
      pkgs = import nixpkgs {
        inherit system;
        overlays = [ rust-overlay.overlays.default ];
      };
      toolchain = pkgs.rust-bin.fromRustupToolchainFile ./rust-toolchain.toml; # shrimply important, describes target and stability branch
    in                                                                         # probably other stuff so it breaks if you replace this with
    {                                                                          # like `pkgs.rust-bin.stable` or whatever i forget .. ! rust
      devShells.${system}.default = pkgs.mkShell rec { # only rec for the LD_LIBRARY_PATH export
        packages = with pkgs; [
          toolchain
          rust-analyzer-unwrapped
          rustup

          # pro tip: xorg
          xorg.libX11
          xorg.libXcursor
          xorg.libXrandr
          xorg.libXi
          xorg.libxcb
          vulkan-loader
          libxkbcommon
          wayland
        ];

        RUST_SRC_PATH = "${toolchain}/lib/rustlib/src/rust/library"; # i forget why this is here .shrug.

        shellHook = ''
          export PATH="/home/$(whoami)/.cargo/bin:$PATH" # shout out to whatever this was for
          export LD_LIBRARY_PATH="$LD_LIBRARY_PATH:${builtins.toString (pkgs.lib.makeLibraryPath packages)}"; # wretched :3
        '';
      };
    };
}
