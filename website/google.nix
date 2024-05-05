{ cert, ... }:
let cert_name = "www.google.com";
in { config, lib, ... }: {
  options = with lib; {
    local_cdn.google = {
      enable = mkEnableOption "google.com local cdn";
      cert = cert.mkOption cert_name;
    };
  };
  config = let cfg = config.local_cdn.google;
  in lib.mkIf cfg.enable {
    local_cdn.certgen.config.servers = lib.mkIf cfg.cert.certgen.enable {
      ${cert_name} = {
        distinguished_name = {
          organization_unit_name = "www.google.com local cdn";
          common_name = "www.google.com";
        };
        subject_alt_names = { dns = [ "www.google.com" ]; };
      };
    };

    services.nginx.virtualHosts."www.google.com" = {
      addSSL = true;
      sslCertificate = cfg.cert.certificate;
      sslCertificateKey = cfg.cert.key;
      locations = {
        "/recaptcha/api.js" = {
          return = "307 https://recaptcha.net/recaptcha/api.js";
          extraConfig = "expires -1;";
        };
      };
    };

    systemd.services.nginx = lib.mkIf cfg.cert.certgen.enable {
      wants = [ cert.certgen.service ];
      after = [ cert.certgen.service ];
    };
  };
}
