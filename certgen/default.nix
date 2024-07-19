{ rustPlatform }:
rustPlatform.buildRustPackage {
  pname = "local_cdn-certgen";
  version = "0.1.0";

  src = ./.;

  cargoLock = {
    lockFileContents = builtins.readFile ./Cargo.lock;
  };
}
