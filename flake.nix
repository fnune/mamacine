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

      # What the core crate needs. No browser engine: it has no interface.
      buildInputsFor = pkgs: [ pkgs.openssl ];

      # Only the window and its tray icon need these.
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

      # The programs the app drives. On Windows they travel with it as sidecars.
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

          # The webview and its dependencies are loaded by name at runtime, so they have to be
          # findable rather than merely linked against. This is deliberately not LD_LIBRARY_PATH:
          # putting Nix's glibc ahead of the host's for every subprocess breaks host tools such as
          # git. `just dev` applies it to the app process alone.
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
