{
  inputs = {
    nixpkgs.url = "github:nixos/nixpkgs/nixos-24.05";
  };

  outputs =
    { nixpkgs, ... }:
    let
      certgen = import ./certgen.nix;
      dns = import ./service/dns.nix;
      cache-proxy = import ./service/cache-proxy.nix;
    in
    {
      packages = {
        x86_64-linux =
          let
            pkgs = nixpkgs.legacyPackages.x86_64-linux;
          in
          {
            local_cdn-certgen = pkgs.callPackage certgen.package { };
            local_cdn-dns = pkgs.callPackage dns.package { };
            local_cdn-proxy = pkgs.callPackage cache-proxy.package { };
          };
      };
      nixosModules = {
        local_cdn =
          args@{ lib, pkgs, ... }:
          let
            local_cdn_lib = {
              cert = certgen.lib lib;
              source = {
                npm = (import ./source/npm.nix) args;
              };
              library = {
                dojo = (import ./library/dojo.nix) args;
                jquery = (import ./library/jquery.nix) args;
                three = import ./library/three.nix;
              };
            };
            importWithLib = p: (import p) local_cdn_lib;
          in
          {
            imports = [
              certgen.module
              (cache-proxy.module { cert = certgen.lib lib; })
              (importWithLib ./website/status.nix)
              (importWithLib ./website/ajax.googleapis.com.nix)
              (importWithLib ./website/google.nix)
            ];
          };
        local_cdn-dns = dns.module;
      };
      formatter.x86_64-linux = nixpkgs.legacyPackages.x86_64-linux.nixfmt-rfc-style;
    };
}
