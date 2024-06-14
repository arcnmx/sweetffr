{
  inputs = {
    nixpkgs = { };
    flakelib.url = "github:flakelib/fl";
    rust = {
      url = "github:arcnmx/nixexprs-rust";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };
  outputs = { self, flakelib, rust, nixpkgs, ... }@inputs: let
    nixlib = nixpkgs.lib;
    callPackageArgs = {
      source = self.lib.crate.src;
      inherit (self.lib.crate) cargoLock;
      inherit (self.lib) crate releaseTag;
      inherit self;
    };
  in flakelib {
    inherit inputs;
    packages = {
      sweetffr = {
        __functor = _: import ./derivation.nix;
        fl'config.args = {
          pkg-config.offset = "build";
          crate.fallback = self.lib.crate;
          releaseTag.fallback = self.lib.releaseTag;
          self.fallback = self;
        };
      };
      sweetffr-w64 = { pkgsCross'mingwW64, rust-w64, source }: pkgsCross'mingwW64.callPackage ./derivation.nix (callPackageArgs // {
        inherit (rust-w64.latest) rustPlatform;
        inherit source;
      });
      sweetffr-static = { pkgsCross'musl64'pkgsStatic, source }: pkgsCross'musl64'pkgsStatic.callPackage ./derivation.nix (callPackageArgs // {
        inherit ((import inputs.rust { pkgs = pkgsCross'musl64'pkgsStatic; }).latest) rustPlatform;
        inherit source;
        enableOpenssl = false;
      });
      default = { sweetffr }: sweetffr;
    };
    devShells = {
      plain = {
        mkShell, writeShellScriptBin, wpexec
      , pkg-config
      , openssl
      , enableRustdoc ? false
      , enableRust ? true, cargo
      , rustTools ? [ ]
      }: mkShell {
        inherit rustTools;
        strictDeps = true;
        buildInputs = [ openssl ];
        nativeBuildInputs = [
          pkg-config
          (writeShellScriptBin "generate" ''
            nix run $FLAKE_ROOT#sweetffr-generate ''${FLAKE_OPTS-} "$@"
          '')
        ] ++ nixlib.optional enableRust cargo;
      };
      stable = { rust'stable, outputs'devShells'plain }: outputs'devShells'plain.override {
        inherit (rust'stable) mkShell;
        enableRust = false;
      };
      dev = { rust'unstable, outputs'devShells'plain }: outputs'devShells'plain.override {
        inherit (rust'unstable) mkShell;
        enableRust = false;
        enableRustdoc = true;
        rustTools = [ "rust-analyzer" ];
      };
      default = { outputs'devShells }: outputs'devShells.plain;
    };
    overlays = {
      sweetffr = final: prev: {
        sweetffr = final.callPackage ./derivation.nix callPackageArgs;
      };
      default = self.overlays.sweetffr;
    };
    legacyPackages = {
      source = { rust'builders }: rust'builders.wrapSource self.lib.crate.src;

      sweetffr-generate = {
        rust'builders
      , sweetffr-readme-github
      , outputHashes
      }: rust'builders.generateFiles {
        name = "readmes";
        paths = {
          ".github/README.md" = sweetffr-readme-github;
          "lock.nix" = outputHashes;
        };
      };
      sweetffr-readme-src = { rust'builders }: rust'builders.wrapSource self.lib.readme-src;
      sweetffr-readme-github = { rust'builders, sweetffr-readme-src }: rust'builders.adoc2md {
        src = "${sweetffr-readme-src}/README.adoc";
        attributes = {
          readme-inc = "${sweetffr-readme-src}/ci/readme/";
          # this file ends up in `.github/README.md`, so its relative links must be adjusted to compensate
          relative-blob = "../";
        };
      };
      sweetffr-pages = { linkFarm, fetchurl }: linkFarm "sweetffr-pages" [
        {
          name = "root/replay.html";
          path = ./pages/replay.html;
        }
      ];
      outputHashes = { rust'builders }: rust'builders.cargoOutputHashes {
        inherit (self.lib) crate;
      };
      rust-w64 = { pkgsCross'mingwW64 }: import inputs.rust { inherit (pkgsCross'mingwW64) pkgs; };
      rust-w64-overlay = { rust-w64 }: let
        target = rust-w64.lib.rustTargetEnvironment {
          inherit (rust-w64) pkgs;
          rustcFlags = [ "-L native=${rust-w64.pkgs.windows.pthreads}/lib" ];
        };
      in cself: csuper: {
        sysroot-std = csuper.sysroot-std ++ [ cself.manifest.targets.${target.triple}.rust-std ];
        cargo-cc = csuper.cargo-cc // cself.context.rlib.cargoEnv {
          inherit target;
        };
        rustc-cc = csuper.rustc-cc // cself.context.rlib.rustcCcEnv {
          inherit target;
        };
      };
    };
    checks = {
      sweetffr = { sweetffr }: sweetffr.override {
        buildType = "debug";
      };
      pages = { sweetffr-pages }: sweetffr-pages;
      rustfmt = { rust'builders, source }: rust'builders.check-rustfmt-unstable {
        src = source;
        config = ./.rustfmt.toml;
      };
      readme-github = { rust'builders, sweetffr-readme-github }: rust'builders.check-generate {
        expected = sweetffr-readme-github;
        src = ./.github/README.md;
        meta.name = "diff .github/README.md (nix run .#sweetffr-generate)";
      };
    };
    lib = {
      crate = rust.lib.importCargo {
        path = ./Cargo.toml;
        inherit (import ./lock.nix) outputHashes;
      };
      inherit (self.lib.crate.package) version;
      releaseTag = "v${self.lib.version}";
      branches = [ "main" ];
      owner = "arcnmx";
      repo = "sweetffr";
      pagesRoot = rust.lib.ghPages {
        inherit (self.lib) owner repo;
      };
      readme-src = let
        whitelist = [
          "/ci"
          "/ci/readme"
          "/ci/readme/content.adoc"
          "/ci/readme/header.adoc"
          "/README.adoc"
        ];
      in builtins.path {
        path = ./.;
        filter = path: type: let
          path' = nixlib.removePrefix (toString ./.) (toString path);
        in builtins.elem path' whitelist;
      };
    };
  };
}
