let package = import ./certgen;
in {
  inherit package;

  lib = lib:
    let
      server_cert_root = ca: "/var/lib/local_cdn/certgen/${ca}/servers";
      ca_cert_root = ca: "/var/lib/local_cdn/certgen/${ca}/ca";
      server_cert = { ca, name }: "${server_cert_root ca}/${name}.pem";
      server_key = { ca, name }: "${server_cert_root ca}/${name}.key";
    in {
      mkOption = with lib;
        { default_ca }:
        mkOption {
          type = types.submodule {
            options = {
              certgen = {
                enable = mkOption {
                  type = types.bool;
                  default = true;
                  description = "use certgen to generate certificate";
                };
                ca = mkOption {
                  type = types.str;
                  default = default_ca;
                  description = "certgen ca name";
                };
              };
              key = mkOption {
                type = types.nullOr types.path;
                default = null;
                description = "private key path";
              };
              certificate = mkOption {
                type = types.nullOr types.path;
                default = null;
                description = "certificate path";
              };
            };
          };
          default = { };
        };

      mkConfig = { name, distinguished_name, subject_alt_names }:
        cfg:
        let
          info = {
            inherit (cfg.certgen) ca;
            inherit name;
          };
        in {
          certgen = lib.mkIf cfg.certgen.enable {
            ${cfg.certgen.ca}.servers.${name} = {
              inherit distinguished_name subject_alt_names;
            };
          };
          certificate =
            if cfg.certgen.enable then server_cert info else cfg.cert;
          key = if cfg.certgen.enable then server_key info else cfg.key;
        };

      certgen = {
        inherit server_cert_root ca_cert_root server_cert server_key;
      };
    };

  module = { config, pkgs, lib, ... }: {
    options = with lib; {
      local_cdn.certgen = {
        enable = mkEnableOption "local_cdn cert";
        user = mkOption { type = types.str; };
        group = mkOption { type = types.str; };
        configs = mkOption {
          type = types.attrsOf (types.submodule {
            options = let
              cert_config = {
                distinguished_name = mkOption {
                  type = types.submodule {
                    options = {
                      organization_unit_name = mkOption {
                        type = types.str;
                        description =
                          "OrganizationUnit of generated certificate";
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
                  default = {
                    dns = [ ];
                    ip_addr = [ ];
                  };
                };
              };
            in {
              overwrite = mkOption {
                type = types.enum [ "always" "expired" "never" ];
                default = "expired";
                description = "Overwrite policy";
              };
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
                description = "ca certificate config";
              };
              servers = mkOption {
                type =
                  types.attrsOf (types.submodule { options = cert_config; });
                default = { };
              };
            };
          });
          default = { };
        };
      };
    };
    config = let cfg = config.local_cdn.certgen;
    in lib.mkIf cfg.enable {
      systemd.services = let
        bin_drv = pkgs.callPackage package { };
        mkService = { configFile }: {
          description = "Server certificate generator for local cdn %I";
          serviceConfig = {
            Type = "oneshot";
            User = cfg.user;
            Group = cfg.group;
            StateDirectory = "local_cdn/certgen/%i";
            ExecStart =
              "${bin_drv}/bin/local_cdn-certgen ${configFile} \${STATE_DIRECTORY}/ca \${STATE_DIRECTORY}/servers \${STATE_DIRECTORY}/state.json";
          };
        };
      in builtins.listToAttrs (builtins.attrValues (builtins.mapAttrs
        (name: config: {
          name = "local_cdn-certgen@${name}";
          value = mkService {
            configFile = pkgs.writeText "certgen-${name}.json"
              (builtins.toJSON {
                inherit (config) overwrite;
                cert = {
                  inherit (config)
                    organization_name expire_secs ca_name ca servers;
                };
              });
          };
        }) cfg.configs));
    };
  };
}
