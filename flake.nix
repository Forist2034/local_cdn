{
  inputs = { nixpkgs.url = "github:nixos/nixpkgs/nixos-23.11"; };

  outputs = { nixpkgs, ... }:
    let
      certgen = import ./certgen;
      dns = import ./service/dns.nix;
    in {
      packages = {
        x86_64-linux = {
          local_cdn-certgen =
            nixpkgs.legacyPackages.x86_64-linux.callPackage certgen.package { };
          local_cdn-dns =
            nixpkgs.legacyPackages.x86_64-linux.callPackage dns.package { };
        };
      };
      nixosModules = {
        local_cdn = args@{ lib, pkgs, ... }:
          let
            local_cdn_lib = {
              cert = certgen.lib lib;
              source = { npm = (import ./source/npm.nix) args; };
              library = {
                dojo = (import ./library/dojo.nix) args;
                jquery = (import ./library/jquery.nix) args;
                three = import ./library/three.nix;
              };
            };
            importWithLib = p: (import p) local_cdn_lib;
          in {
            imports = [
              certgen.module
              (importWithLib ./website/status.nix)
              (importWithLib ./website/ajax.googleapis.com.nix)
            ];
          };
        local_cdn-dns = dns.module;
      };
    };
}
