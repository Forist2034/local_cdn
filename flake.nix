{
  inputs = { nixpkgs.url = "github:nixos/nixpkgs/nixos-23.11"; };

  outputs = { nixpkgs, ... }:
    let certgen = import ./certgen;
    in {
      packages = {
        x86_64-linux = {
          local_cdn-certgen =
            nixpkgs.legacyPackages.x86_64-linux.callPackage certgen.package { };
        };
      };
      nixosModules = {
        local_cdn = args@{ lib, pkgs, ... }:
          let
            local_cdn_lib = {
              cert = certgen.lib lib;
              source = { npm = (import ./source/npm) args; };
              library = {
                dojo = (import ./library/dojo) args;
                jquery = (import ./library/jquery) args;
                three = import ./library/three.js;
              };
            };
            importWithLib = p: (import p) local_cdn_lib;
          in {
            imports = [
              certgen.module
              (importWithLib ./website/status)
              (importWithLib ./website/ajax.googleapis.com)
            ];
          };
      };
    };
}
