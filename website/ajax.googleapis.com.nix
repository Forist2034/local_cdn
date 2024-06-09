{ cert, source, library, ... }:
{ config, pkgs, lib, ... }:
let cert_name = "ajax.googleapis.com";
in {
  options = with lib; {
    local_cdn.googleapis.ajax = {
      enable = mkEnableOption "ajax.googleapis.com local cdn";
      cert = cert.mkOption { default_ca = "static"; };
    };
  };

  config = let cfg = config.local_cdn.googleapis.ajax;
  in lib.mkIf cfg.enable (let
    cert_config = cert.mkConfig {
      name = cert_name;
      distinguished_name = {
        organization_unit_name = "ajax.googleapis.com local cdn";
        common_name = "ajax.googleapis.com";
      };
      subject_alt_names = { dns = [ "ajax.googleapis.com" ]; };
    } cfg.cert;
  in {
    local_cdn.certgen.configs = cert_config.certgen;

    services.nginx.virtualHosts."ajax.googleapis.com" = let
      loadFile = p: builtins.fromJSON (builtins.readFile p);
      linkNpmVersions = desc_path: vs:
        let desc = loadFile desc_path;
        in pkgs.runCommand "ajax-googleapis-${desc.drv_name}" { } (''
          mkdir $out
        '' + builtins.concatStringsSep "\n" (builtins.map (v:
          if builtins.hasAttr v.version desc.versions then
            let
              subdir = if v.dir == null then "" else v.dir;
              package = source.npm.fetchNpm desc.versions.${v.version};
            in "ln -sv ${package}/${subdir} $out/${v.version}"
          else
            "") vs));
      npmVersions = loadFile ../data/website/ajax.googleapis.com/npm.json;
    in {
      addSSL = true;
      sslCertificate = cert_config.certificate;
      sslCertificateKey = cert_config.key;
      locations = let
        mkLocation = p: {
          alias = p + "/";
          extraConfig = "expires -1;";
        };
        mkLinkedNpmLoc = p: vs: mkLocation (linkNpmVersions p vs);
      in {
        "/ajax/libs/cesiumjs/1.78/" = mkLocation ((source.npm.fetchNpm
          (loadFile ../data/source/npm/cesium.json).versions."1.78.0"));
        "/ajax/libs/d3js/" =
          mkLinkedNpmLoc ../data/source/npm/d3.json npmVersions.d3;
        "/ajax/libs/dojo/" = mkLocation
          (pkgs.runCommand "ajax-googleapis-dojo" { } (''
            mkdir $out
          '' + builtins.concatStringsSep "\n" (builtins.map (v:
            if builtins.hasAttr v library.dojo.packages then
              "ln -sv ${library.dojo.packages.${v}} $out/${v}"
            else
              "") (loadFile ../data/website/ajax.googleapis.com/dojo.json))));
        "/ajax/libs/hammerjs/" =
          mkLinkedNpmLoc ../data/source/npm/hammerjs.json npmVersions.hammerjs;
        "/ajax/libs/indefinite-observable/" =
          mkLinkedNpmLoc ../data/source/npm/indefinite-observable.json
          npmVersions.indefinite-observable;
        "/ajax/libs/jquery/" = let jquery = library.jquery.core;
        in mkLocation (pkgs.runCommand "ajax-googleapis-jquery" { } (''
          mkdir $out
        '' + builtins.concatStringsSep "\n" (builtins.map (v:
          if builtins.hasAttr v jquery.packages then
            ''
              mkdir $out/${v}
            '' + builtins.concatStringsSep "\n" ((builtins.map (file:
              "ln -sv ${file.package} $out/${v}/jquery.${file.extension}"))
              (builtins.attrValues jquery.packages.${v}))
          else
            "") (loadFile ../data/website/ajax.googleapis.com/jquery.json))));
        #"/ajax/libs/jqueryui/" = ;
        #"/ajax/libs/jquerymobile/" = "";
        "/ajax/libs/listjs/" =
          mkLinkedNpmLoc ../data/source/npm/list.js.json npmVersions."list.js";
        "/ajax/libs/material-motion/" =
          mkLinkedNpmLoc ../data/source/npm/material-motion.json
          npmVersions.material-motion;
        "/ajax/libs/model-viewer/" =
          mkLinkedNpmLoc (../data/source/npm + "/@google/model-viewer.json")
          npmVersions."@google/model-viewer";
        "/ajax/libs/shaka-player/" =
          mkLinkedNpmLoc ../data/source/npm/shaka-player.json
          npmVersions.shaka-player;
        "/ajax/libs/spf/" =
          mkLinkedNpmLoc ../data/source/npm/spf.json npmVersions.spf;
        "/ajax/libs/threejs/" = let
          npm = loadFile ../data/source/npm/three.json;
          version_map = library.three.version_map;
        in mkLocation (pkgs.runCommand "ajax-googleapis-three" { } (''
          mkdir $out
        '' + builtins.concatStringsSep "\n" (builtins.map (v:
          if builtins.hasAttr v.version version_map then
            let
              npm_ver = version_map.${v.version};
              package = source.npm.fetchNpm npm.versions.${npm_ver};
              dir = if v.dir == null then "" else v.dir;
            in "ln -sv ${package}/${dir} $out/${v.version}"
          else
            "") (loadFile ../data/website/ajax.googleapis.com/three.json))));
        "/ajax/libs/webfont/" =
          mkLinkedNpmLoc ../data/source/npm/webfontloader.json
          npmVersions.webfontloader;
      };
    };
  });
}
