{ cert, ... }:
{
  config,
  pkgs,
  lib,
  ...
}:
{
  options = with lib; {
    local_cdn.status = {
      enable = mkEnableOption "status page";
      certgen = mkOption {
        type = types.listOf types.str;
        default = [ ];
        description = "certgen configs";
      };
    };
  };

  config =
    let
      cfg = config.local_cdn.status;
    in
    lib.mkIf cfg.enable {
      services.nginx.virtualHosts.local_cdn-status = {
        locations =
          {
            "/status".extraConfig = ''
              stub_status;
            '';
            "/up".return = "204";
          }
          // builtins.listToAttrs (
            builtins.map (name: {
              name = "/ca/${name}/";
              value = {
                alias = cert.certgen.ca_cert_root name + "/";
                extraConfig = ''
                  autoindex on;
                '';
              };
            }) cfg.certgen
          );
      };
    };
}
