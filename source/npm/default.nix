{ pkgs, ... }: {
  fetchNpm = p:
    let tarball = pkgs.fetchurl { inherit (p) url hash; };
    in pkgs.runCommand p.drv_name {
      outputHashMode = "recursive";
      outputHashAlgo = "sha256";
      __contentAddressed = true;
    } ''
      mkdir $out
      tar xf ${tarball} -C $out --strip-components=1
    '';
}
