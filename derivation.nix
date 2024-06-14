{
  lib
, rustPlatform
, hostPlatform
, pkg-config
, openssl
, buildType ? "release"
, cargoLock ? crate.cargoLock
, source ? crate.src
, crate ? self.lib.crate
, self ? import ./. { pkgs = null; system = null; }
, releaseTag ? self.lib.releaseTag
, enableRecent ? true
, enableOpenssl ? hostPlatform.isLinux
}: let
  inherit (lib.lists) optional;
  inherit (crate.package) version;
  pname = "sweetffr";
in rustPlatform.buildRustPackage rec {
  inherit pname version
    cargoLock buildType;
  src = source;

  SWEETFFR_RELEASE_TAG = releaseTag;

  buildNoDefaultFeatures = true;
  buildFeatures = [ ]
  ++ optional enableRecent "recent"
  ++ optional enableOpenssl "openssl"
  ++ optional (!enableOpenssl) "sha1_smol";

  buildInputs = optional hostPlatform.isLinux openssl;
  nativeBuildInputs = [ pkg-config ];

  passthru = {
    input = self;
  };

  meta = {
    description = "Discord rich presence for FlashFlashRevolution";
    homepage = "https://github.com/arcnmx/sweetffr";
    license = lib.licenses.mit;
    maintainers = [ lib.maintainers.arcnmx ];
    mainProgram = pname;
  };
}
