let
  package = { rustPlatform }:
    rustPlatform.buildRustPackage {
      pname = "local_cdn-certgen";
      version = "0.1.0";

      src = ./.;

      cargoLock = { lockFileContents = builtins.readFile ./Cargo.lock; };
    };
in {
  inherit package;

  lib = lib:
    let
      server_cert_root = "/var/lib/local_cdn/certgen/servers";
      ca_cert_root = "/var/lib/local_cdn/certgen/ca";
    in {
      mkOption = with lib;
        name:
        mkOption {
          type = types.submodule {
            options = {
              certgen.enable = mkOption {
                type = types.bool;
                default = true;
                description = "use certgen to generate certificate";
              };
              key = mkOption {
                type = types.path;
                description = "private key path";
              };
              certificate = mkOption {
                type = types.path;
                description = "certificate path";
              };
            };
          };
          default = {
            certgen.enable = true;
            key = "${server_cert_root}/${name}.key";
            certificate = "${server_cert_root}/${name}.pem";
          };
        };
      certgen = {
        inherit server_cert_root ca_cert_root;
        service = "local_cdn-certgen.service";
      };
    };

  module = { config, pkgs, lib, ... }: {
    options = with lib; {
      local_cdn.certgen = {
        enable = mkEnableOption "local_cdn cert";
        user = mkOption { type = types.str; };
        group = mkOption { type = types.str; };
        overwrite = mkOption {
          type = types.enum [ "always" "expired" "never" ];
          default = "expired";
          description = "Overwrite policy";
        };
        config = let
          cert_config = {
            distinguished_name = mkOption {
              type = types.submodule {
                options = {
                  organization_unit_name = mkOption {
                    type = types.str;
                    description = "OrganizationUnit of generated certificate";
                  };
                  common_name = mkOption {
                    type = types.str;
                    description = "CommonName of generated certificate";
                  };
                };
              };
            };
            subject_alt_names = mkOption {
              type = types.submodule {
                options = {
                  dns = mkOption {
                    type = types.listOf types.str;
                    default = [ ];
                  };
                  ip_addr = mkOption {
                    type = types.listOf types.str;
                    default = [ ];
                  };
                };
              };
            };
          };
        in mkOption {
          type = types.submodule {
            options = {
              organization_name = mkOption {
                type = types.str;
                default = "local cdn";
                description = "OrganizationName of generated certificate";
              };
              expire_secs = mkOption {
                type = types.ints.positive;
                default = 7 * 24 * 60 * 30; # 7 days
                description = "certificate validity time in seconds";
              };
              ca_name = mkOption {
                type = types.str;
                default = "ca";
                description = "ca certificate name";
              };
              ca = mkOption {
                type = types.submodule { options = cert_config; };
                default = {
                  distinguished_name = {
                    organization_unit_name = "local cdn ca";
                    common_name = "ca";
                  };
                  subject_alt_names = { };
                };
                description = "ca certificate config";
              };
              servers = mkOption {
                type =
                  types.attrsOf (types.submodule { options = cert_config; });
                default = { };
              };
            };
          };
        };
      };
    };

    config = let cfg = config.local_cdn.certgen;
    in lib.mkIf cfg.enable {
      systemd.services."local_cdn-certgen" = let
        bin_drv = pkgs.callPackage package { };
        config_file = pkgs.writeText "certgen.json" (builtins.toJSON {
          inherit (cfg) overwrite;
          cert = cfg.config;
        });
      in {
        description = "Server certificate generator for local cdn";
        serviceConfig = {
          Type = "oneshot";
          User = cfg.user;
          Group = cfg.group;
          StateDirectory = "local_cdn/certgen";
          ExecStart =
            "${bin_drv}/bin/local_cdn-certgen ${config_file} \${STATE_DIRECTORY}/ca \${STATE_DIRECTORY}/servers \${STATE_DIRECTORY}/state.json";
        };
      };
    };
  };
}
