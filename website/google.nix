{ cert, ... }:
let cert_name = "www.google.com";
in { config, lib, ... }: {
  options = with lib; {
    local_cdn.google = {
      enable = mkEnableOption "google.com local cdn";
      cert = cert.mkOption { default_ca = "static"; };
    };
  };
  config = let cfg = config.local_cdn.google;
  in lib.mkIf cfg.enable (let
    cert_config = cert.mkConfig {
      name = cert_name;
      distinguished_name = {
        organization_unit_name = "www.google.com local cdn";
        common_name = "www.google.com";
      };
      subject_alt_names = { dns = [ "www.google.com" ]; };
    } cfg.cert;
  in {
    local_cdn.certgen.configs = cert_config.certgen;

    services.nginx.virtualHosts."www.google.com" = {
      addSSL = true;
      sslCertificate = cert_config.certificate;
      sslCertificateKey = cert_config.key;
      locations = {
        "/recaptcha/api.js" = {
          return = "307 https://recaptcha.net/recaptcha/api.js";
          extraConfig = "expires -1;";
        };
      };
    };
  });
}
