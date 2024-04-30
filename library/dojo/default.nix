{ pkgs, ... }:
let
  versions =
    builtins.fromJSON (builtins.readFile ../../data/library/dojo/package.json);
in {
  inherit versions;
  packages = builtins.mapAttrs (version: info:
    let tarball = pkgs.fetchurl { inherit (info) url md5; };
    in pkgs.runCommand "dojo-${version}" {
      outputHashMode = "recursive";
      outputHashAlgo = "sha256";
      __contentAddressed = true;
    } ''
      mkdir $out
      tar xf ${tarball} -C $out --strip-components=1
    '') versions;
}
