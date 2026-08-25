{
  description = "Mamá Cine: one-click films for someone who should not have to learn Usenet";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs =
    { self, nixpkgs, rust-overlay }:
    let
      systems = [ "x86_64-linux" "aarch64-linux" ];

      forAllSystems =
        f:
        nixpkgs.lib.genAttrs systems (
          system:
          f (import nixpkgs {
            inherit system;
            overlays = [ rust-overlay.overlays.default ];
            config.allowUnfreePredicate = pkg: nixpkgs.lib.getName pkg == "unrar";
          })
        );

      toolchainFor = pkgs: pkgs.rust-bin.fromRustupToolchainFile ./rust-toolchain.toml;

      nativeBuildInputsFor = pkgs: [ pkgs.pkg-config ];

      buildInputsFor = pkgs: [ pkgs.openssl ];

      desktopInputsFor = pkgs: [
        pkgs.gdk-pixbuf
        pkgs.glib
        pkgs.glib-networking
        pkgs.gtk3
        pkgs.libayatana-appindicator
        pkgs.libsoup_3
        pkgs.librsvg
        pkgs.webkitgtk_4_1
      ];

      toolsFor = pkgs: [ pkgs.ffmpeg-headless pkgs.nzbget pkgs.p7zip pkgs.unrar ];
    in
    {
      devShells = forAllSystems (pkgs: {
        default = pkgs.mkShell {
          packages =
            [ (toolchainFor pkgs) pkgs.cargo-tauri pkgs.imagemagick pkgs.just pkgs.nodejs ]
            ++ nativeBuildInputsFor pkgs
            ++ buildInputsFor pkgs
            ++ desktopInputsFor pkgs
            ++ toolsFor pkgs;

          shellHook = ''
            export MAMACINE_LIBRARY_PATH="${pkgs.lib.makeLibraryPath (desktopInputsFor pkgs)}"
            export GIO_MODULE_DIR="${pkgs.glib-networking}/lib/gio/modules"
            export RUST_BACKTRACE=1
          '';
        };
      });

      formatter = forAllSystems (pkgs: pkgs.nixfmt-tree);
    };
}
