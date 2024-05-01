{ cert, ... }:
{ config, pkgs, lib, ... }: {
  options = with lib; {
    local_cdn.status = {
      enable = mkEnableOption "status page";
      ca_path = mkOption {
        type = types.path;
        description = "ca certificates path";
        default = cert.certgen.ca_cert_root;
      };
    };
  };

  config = let cfg = config.local_cdn.status;
  in lib.mkIf cfg.enable {
    services.nginx.virtualHosts.local_cdn-status = {
      locations = {
        "/ca/" = {
          alias = cfg.ca_path + "/";
          extraConfig = ''
            autoindex on;
          '';
        };
        "/status".extraConfig = ''
          stub_status;
        '';
        "/up".return = "204";
      };
    };
  };
}
