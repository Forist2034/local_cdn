let
  package = import ./cache-proxy;
in
{
  inherit package;
  module =
    { cert, ... }:
    {
      config,
      lib,
      pkgs,
      ...
    }:
    {
      options = with lib; {
        local_cdn.proxy = {
          enable = mkEnableOption "enable";
          user = mkOption {
            type = types.str;
            description = "User account that proxy runs";
          };
          group = mkOption {
            type = types.str;
            description = "User group that proxy runs";
          };
          servers = mkOption {
            type = types.attrsOf (
              types.submodule {
                options = {
                  cert = cert.mkOption { default_ca = "proxy"; };
                };
              }
            );
            default = { };
          };
        };
      };

      config =
        let
          cfg = config.local_cdn.proxy;
        in
        lib.mkIf cfg.enable (
          let
            servers = builtins.mapAttrs (domain: config: {
              cert_config = cert.mkConfig {
                name = domain;
                distinguished_name = {
                  organization_unit_name = "local cdn proxy ${domain}";
                  common_name = domain;
                };
                subject_alt_names.dns = [ domain ];
              } config.cert;
            }) cfg.servers;
          in
          {
            systemd.services."local_cdn-proxy@" =
              let
                bin_drv = pkgs.callPackage package { };
              in
              {
                description = "local cdn caching proxy for %I";
                serviceConfig = {
                  ExecStart = ''
                    ${bin_drv}/bin/local_cdn-proxy \
                      --log-output journal \
                      --unix "''${RUNTIME_DIRECTORY}/proxy.sock" \
                      ''${CACHE_DIRECTORY} \
                      %i
                  '';
                  RuntimeDirectory = [ "local_cdn/proxy/%i" ];
                  CacheDirectory = [ "local_cdn/proxy/%i" ];

                  ProtectProc = "noaccess";
                  ProcSubset = "pid";

                  User = cfg.user;
                  Group = cfg.group;

                  CapabilityBoundingSet = [ "" ];
                  NoNewPrivileges = true;

                  ProtectSystem = "strict";
                  ProtectHome = true;
                  PrivateTmp = true;
                  PrivateDevices = true;
                  PrivateIPC = true;
                  PrivateUsers = true;
                  ProtectHostname = true;
                  ProtectClock = true;
                  ProtectKernelTunables = true;
                  ProtectKernelModules = true;
                  ProtectKernelLogs = true;
                  ProtectControlGroups = true;
                  RestrictAddressFamilies = [
                    "AF_UNIX"
                    "AF_INET"
                    "AF_INET6"
                  ];
                  RestrictNamespaces = true;
                  LockPersonality = true;
                  MemoryDenyWriteExecute = true;
                  RestrictRealtime = true;
                  RestrictSUIDSGID = true;
                  RemoveIPC = true;

                  SystemCallArchitectures = "native";
                };
              };

            local_cdn.certgen.configs = lib.mkMerge (
              builtins.map (cfg: cfg.cert_config.certgen) (builtins.attrValues servers)
            );

            services.nginx.virtualHosts = builtins.mapAttrs (domain: cfg: {
              addSSL = true;
              sslCertificate = cfg.cert_config.certificate;
              sslCertificateKey = cfg.cert_config.key;
              locations."/" = {
                proxyPass = "http://unix:/run/local_cdn/proxy/${domain}/proxy.sock:";
                extraConfig = "proxy_set_header Host $host;";
              };
            }) servers;
          }
        );
    };
}
