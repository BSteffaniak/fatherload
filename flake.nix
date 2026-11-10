{
  description = "fatherload development environment";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };
    cargoMacheteSrc = {
      url = "github:BSteffaniak/cargo-machete/ignored-dirs";
      flake = false;
    };
  };

  outputs =
    {
      self,
      nixpkgs,
      flake-utils,
      rust-overlay,
      cargoMacheteSrc,
    }:
    flake-utils.lib.eachDefaultSystem (
      system:
      let
        overlays = [ (import rust-overlay) ];
        pkgs = import nixpkgs { inherit system overlays; };
        isDarwin = pkgs.stdenv.hostPlatform.isDarwin;
        isLinux = pkgs.stdenv.hostPlatform.isLinux;
        cargoMachete = pkgs.rustPlatform.buildRustPackage {
          pname = "cargo-machete";
          version = "ignored-dirs";
          src = cargoMacheteSrc;
          cargoLock = {
            lockFile = "${cargoMacheteSrc}/Cargo.lock";
          };
          doCheck = false;
        };
        rustToolchain = pkgs.rust-bin.fromRustupToolchainFile ./rust-toolchain.toml;
        linuxGraphicsLibraries = pkgs.lib.optionals isLinux (
          with pkgs;
          [
            libGL
            wayland
            xorg.libX11
            xorg.libXcursor
            xorg.libXi
            xorg.libXinerama
            xorg.libXrandr
          ]
        );
      in
      {
        devShells.web = pkgs.mkShell {
          packages = [
            (rustToolchain.override { targets = [ "wasm32-unknown-emscripten" ]; })
            pkgs.emscripten
            pkgs.cmake
            pkgs.python3
            pkgs.nodejs
            pkgs.llvmPackages.libclang
          ];
          LIBCLANG_PATH = "${pkgs.llvmPackages.libclang.lib}/lib";
          shellHook = ''
            # Emscripten's Nix store cache is read-only; keep generated ports local.
            export EM_CACHE="$PWD/target/emscripten-cache"
            unset SDKROOT MACOSX_DEPLOYMENT_TARGET RUSTFLAGS CFLAGS CXXFLAGS LDFLAGS BINDGEN_EXTRA_CLANG_ARGS
            echo "Fatherload web shell: scripts/build-web.sh"
          '';
        };
        devShells.default = pkgs.mkShell {
          buildInputs = [
            rustToolchain
            cargoMachete
            pkgs.cargo-deny
            pkgs.cmake
            pkgs.pkg-config
            pkgs.llvmPackages.libclang
            pkgs.fish
          ]
          ++ pkgs.lib.optionals (!isDarwin) [
            pkgs.clang
          ]
          ++ linuxGraphicsLibraries;

          LIBCLANG_PATH = "${pkgs.llvmPackages.libclang.lib}/lib";
          RUST_BACKTRACE = "1";

          shellHook = ''
            if [ "$(uname -s)" = "Darwin" ]; then
              export SDKROOT="$(env -u DEVELOPER_DIR -u SDKROOT /usr/bin/xcrun --sdk macosx --show-sdk-path)"
              export MACOSX_DEPLOYMENT_TARGET="''${MACOSX_DEPLOYMENT_TARGET:-11.0}"

              export CC="$(env -u DEVELOPER_DIR -u SDKROOT /usr/bin/xcrun --sdk macosx --find clang)"
              export CXX="$(env -u DEVELOPER_DIR -u SDKROOT /usr/bin/xcrun --sdk macosx --find clang++)"

              unset DEVELOPER_DIR
              unset NIX_LDFLAGS
              unset NIX_LDFLAGS_FOR_TARGET
              unset NIX_CFLAGS_COMPILE
              unset NIX_CFLAGS_COMPILE_FOR_TARGET
              unset CMAKE_FRAMEWORK_PATH
              unset CMAKE_INCLUDE_PATH
              unset CMAKE_LIBRARY_PATH
              unset DYLD_FALLBACK_LIBRARY_PATH
              unset DYLD_LIBRARY_PATH
              unset PKG_CONFIG_PATH

              export CARGO_TARGET_AARCH64_APPLE_DARWIN_LINKER="$CC"
              export CARGO_TARGET_X86_64_APPLE_DARWIN_LINKER="$CC"

              export CFLAGS="-isysroot $SDKROOT -mmacosx-version-min=$MACOSX_DEPLOYMENT_TARGET ''${CFLAGS:-}"
              export CXXFLAGS="-isysroot $SDKROOT -mmacosx-version-min=$MACOSX_DEPLOYMENT_TARGET ''${CXXFLAGS:-}"
              export LDFLAGS="-isysroot $SDKROOT -mmacosx-version-min=$MACOSX_DEPLOYMENT_TARGET ''${LDFLAGS:-}"

              export RUSTFLAGS="-C link-arg=-isysroot -C link-arg=$SDKROOT -C link-arg=-mmacosx-version-min=$MACOSX_DEPLOYMENT_TARGET ''${RUSTFLAGS:-}"

              export LIBRARY_PATH="$SDKROOT/usr/lib"
              export BINDGEN_EXTRA_CLANG_ARGS="--sysroot=$SDKROOT -isysroot $SDKROOT ''${BINDGEN_EXTRA_CLANG_ARGS:-}"
            fi

            echo "fatherload development environment loaded"
            echo "Available tools:"
            echo "  - cargo ($(cargo --version))"
            echo "  - rustc ($(rustc --version))"
            echo "  - clippy ($(cargo clippy --version))"
            echo "  - cargo-deny ($(cargo deny --version))"
            echo "  - cargo-machete ($(cargo machete --version))"
            echo "  - cmake ($(cmake --version | head -1))"

            # Only exec fish if we're in an interactive shell (not running a command)
            if [ -z "$IN_NIX_SHELL_FISH" ] && [ -z "$BASH_EXECUTION_STRING" ]; then
              case "$-" in
                *i*) export IN_NIX_SHELL_FISH=1; exec fish ;;
              esac
            fi
          '';
        };
      }
    );
}
