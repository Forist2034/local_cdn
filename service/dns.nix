let
  package = import ./dns;
in
{
  inherit package;
  module =
    {
      config,
      pkgs,
      lib,
      ...
    }:
    let
      configFormat = pkgs.formats.json { };
    in
    {
      options = {
        local_cdn.dns = with lib; {
          enable = mkEnableOption "local cdn dns";
          config = mkOption { type = configFormat.type; };
        };
      };
      config =
        let
          cfg = config.local_cdn.dns;
        in
        lib.mkIf cfg.enable {
          systemd.services.local_cdn-dns =
            let
              bin_drv = pkgs.callPackage package { };
              config_file = configFormat.generate "local_cdn-dns.json" cfg.config;
            in
            {
              description = "local cdn dns";
              serviceConfig = {
                ExecStart = "${bin_drv}/bin/local_cdn-dns --log-output journal ${config_file}";

                ProtectProc = "noaccess";
                ProcSubset = "pid";
                DynamicUser = true;

                CapabilityBoundingSet = [ "CAP_NET_BIND_SERVICE" ];
                AmbientCapabilities = [ "CAP_NET_BIND_SERVICE" ];
                NoNewPrivileges = true;

                ProtectSystem = "strict";
                ProtectHome = true;
                PrivateTmp = true;
                PrivateDevices = true;
                PrivateIPC = true;
                ProtectHostname = true;
                ProtectClock = true;
                ProtectKernelTunables = true;
                ProtectKernelModules = true;
                ProtectKernelLogs = true;
                ProtectControlGroups = true;
                RestrictAddressFamilies = [
                  "AF_INET"
                  "AF_INET6"
                  "AF_UNIX"
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
        };
    };
}
