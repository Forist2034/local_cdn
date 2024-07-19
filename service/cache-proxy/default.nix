{ rustPlatform }:
rustPlatform.buildRustPackage {
  pname = "local_cdn-proxy";
  version = "0.1.0";

  src = ./.;

  cargoLock = {
    lockFileContents = builtins.readFile ./Cargo.lock;
  };
}
