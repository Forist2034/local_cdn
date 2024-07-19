{ pkgs, ... }:
let
  loadFile = p: builtins.fromJSON (builtins.readFile p);
in
{
  core =
    let
      versions = loadFile ../data/library/jquery/core.json;
      extension = {
        original = "js";
        min = "min.js";
        pack = "pack.js";
        slim = "slim.js";
        slim_min = "slim.min.js";
        module = "module.js";
        module_min = "module.min.js";
        slim_module = "slim.module.js";
        slim_module_min = "slim.module.min.js";
      };
    in
    {
      inherit versions extension;
      packages = builtins.mapAttrs (
        v: files:
        builtins.mapAttrs (var: pkg: {
          package = pkgs.fetchurl { inherit (pkg) url hash; };
          extension = extension.${var};
        }) files
      ) versions;
    };
}
